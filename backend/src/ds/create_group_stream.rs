// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! APQ group creation on one stream: the client authenticates, gets fresh
//! group ids, and creates the group with exactly these ids.

use std::{collections::HashMap, sync::Arc};

use aircommon::{
    credentials::keys::ClientVerifyingKey,
    crypto::signatures::signable::Verifiable,
    identifiers::{QualifiedGroupId, UserId},
};
use airprotos::{
    common::v1::ClientMetadata,
    convert::RefInto,
    delivery_service::v1::{
        CreateApqGroupPayload, CreateGroupStreamBegin, CreateGroupStreamBeginPayload,
        CreateGroupStreamCreated, CreateGroupStreamRequest, CreateGroupStreamReserved,
        CreateGroupStreamResponse, ProvisionAttachmentResponse, create_group_stream_request,
        create_group_stream_response,
    },
    signed::SignedRequest,
};
use futures_util::{Stream, StreamExt};
use metrics::{counter, describe_counter};
use parking_lot::Mutex;
use tokio::{
    sync::mpsc,
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tonic::{Code, Status};
use tracing::debug;

use crate::settings::DsSettings;

const METRIC_BEGUN: &str = "air_ds_create_group_streams_begun_total";
const METRIC_CREATED: &str = "air_ds_create_group_streams_created_total";
const METRIC_TIMED_OUT: &str = "air_ds_create_group_streams_timed_out_total";
const METRIC_REJECTED: &str = "air_ds_create_group_streams_rejected_total";

pub(super) fn describe_metrics() {
    describe_counter!(
        METRIC_BEGUN,
        "Group creation streams that handed out group ids"
    );
    describe_counter!(
        METRIC_CREATED,
        "Group creation streams that created their group"
    );
    describe_counter!(
        METRIC_TIMED_OUT,
        "Group creation streams closed at the deadline, by the step they waited for"
    );
    describe_counter!(
        METRIC_REJECTED,
        "Group creation streams the server rejected, by cause"
    );
}

/// The user a stream authenticated in its begin step.
#[derive(Debug, Clone)]
pub(super) struct StreamCreator {
    pub(super) user_id: UserId,
    pub(super) verifying_key: ClientVerifyingKey,
}

/// The group ids a stream handed out.
#[derive(Debug, Clone)]
pub(super) struct StreamReservation {
    pub(super) t_group_id: QualifiedGroupId,
    pub(super) pq_group_id: QualifiedGroupId,
}

/// What the stream protocol needs from the DS.
pub(super) trait CreateGroupStreamBackend {
    fn settings(&self) -> &DsSettings;

    fn verify_client_version(&self, client_metadata: Option<&ClientMetadata>)
    -> Result<(), Status>;

    /// Returns the key the AS stores for `user_id`, or `None` for an unknown
    /// user.
    fn user_verifying_key(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<ClientVerifyingKey>, Status>> + Send;

    /// Consumes one unit of the user's creation allowance. Fails when none is
    /// left.
    fn consume_creation_allowance(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<(), Status>> + Send;

    fn fresh_group_id(&self) -> QualifiedGroupId;

    /// Returns `None` when no storage is configured.
    fn provision_group_profile(
        &self,
        size: i64,
    ) -> impl Future<Output = Result<Option<ProvisionAttachmentResponse>, Status>> + Send;

    fn create_apq_group(
        &self,
        creator: &StreamCreator,
        reservation: &StreamReservation,
        payload: CreateApqGroupPayload,
    ) -> impl Future<Output = Result<(), Status>> + Send;
}

/// Authenticated creation streams, per user and in total.
#[derive(Debug, Clone, Default)]
pub(super) struct CreateGroupStreams {
    open: Arc<Mutex<OpenStreams>>,
}

#[derive(Debug, Default)]
struct OpenStreams {
    total: usize,
    per_user: HashMap<UserId, usize>,
}

impl CreateGroupStreams {
    /// Counts a new stream of `user_id` against the process and the user.
    fn acquire(
        &self,
        user_id: &UserId,
        max_streams: usize,
        max_per_user: usize,
    ) -> Result<StreamSlot, StreamError> {
        let mut open = self.open.lock();
        if open.total >= max_streams {
            return Err(StreamError::Rejected(
                RejectCause::ProcessLimit,
                Status::resource_exhausted(
                    "Too many group creation streams, please try again later",
                ),
            ));
        }
        let count = open.per_user.get(user_id).copied().unwrap_or(0);
        if count >= max_per_user {
            return Err(StreamError::Rejected(
                RejectCause::UserLimit,
                Status::resource_exhausted(
                    "Too many open group creation streams, please try again later",
                ),
            ));
        }
        open.total += 1;
        open.per_user.insert(user_id.clone(), count + 1);
        Ok(StreamSlot {
            open: self.open.clone(),
            user_id: user_id.clone(),
        })
    }

    #[cfg(test)]
    fn counts(&self, user_id: &UserId) -> (usize, usize) {
        let open = self.open.lock();
        (open.total, open.per_user.get(user_id).copied().unwrap_or(0))
    }
}

/// An authenticated stream, released on drop.
#[derive(Debug)]
pub(super) struct StreamSlot {
    open: Arc<Mutex<OpenStreams>>,
    user_id: UserId,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut open = self.open.lock();
        open.total -= 1;
        if let Some(count) = open.per_user.get_mut(&self.user_id) {
            *count -= 1;
            if *count == 0 {
                open.per_user.remove(&self.user_id);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RejectCause {
    ProcessLimit,
    UserLimit,
    Version,
    Unauthenticated,
    RateLimited,
    Protocol,
    InvalidCreate,
    Internal,
    Shutdown,
}

impl RejectCause {
    fn as_str(self) -> &'static str {
        match self {
            RejectCause::ProcessLimit => "process_limit",
            RejectCause::UserLimit => "user_limit",
            RejectCause::Version => "version",
            RejectCause::Unauthenticated => "unauthenticated",
            RejectCause::RateLimited => "rate_limited",
            RejectCause::Protocol => "protocol",
            RejectCause::InvalidCreate => "invalid_create",
            RejectCause::Internal => "internal",
            RejectCause::Shutdown => "shutdown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum WaitingFor {
    /// The server's own checks between the begin step and the reserved ids.
    Admission,
    Create,
}

impl WaitingFor {
    fn as_str(self) -> &'static str {
        match self {
            WaitingFor::Admission => "admission",
            WaitingFor::Create => "create",
        }
    }
}

#[derive(Debug)]
enum StreamError {
    TimedOut(WaitingFor),
    Rejected(RejectCause, Status),
}

impl StreamError {
    fn protocol(message: &'static str) -> Self {
        Self::Rejected(RejectCause::Protocol, Status::invalid_argument(message))
    }

    fn unauthenticated() -> Self {
        Self::Rejected(
            RejectCause::Unauthenticated,
            Status::unauthenticated("unauthenticated"),
        )
    }

    fn internal(status: Status) -> Self {
        Self::Rejected(RejectCause::Internal, status)
    }

    /// Rejects with `cause`, or as internal for an internal error.
    fn classify(status: Status, cause: RejectCause) -> Self {
        let cause = match status.code() {
            Code::Internal => RejectCause::Internal,
            _ => cause,
        };
        Self::Rejected(cause, status)
    }
}

enum Outcome {
    Created,
    /// The client went away before the stream finished.
    Closed,
}

/// Runs the protocol of one stream and sends its outcome to `outgoing`.
pub(super) async fn run_protocol(
    backend: &impl CreateGroupStreamBackend,
    streams: &CreateGroupStreams,
    stop: &CancellationToken,
    incoming: impl Stream<Item = Result<CreateGroupStreamRequest, Status>> + Unpin,
    outgoing: mpsc::Sender<Result<CreateGroupStreamResponse, Status>>,
) {
    match run_protocol_impl(backend, streams, stop, incoming, &outgoing).await {
        Ok(Outcome::Created) => counter!(METRIC_CREATED).increment(1),
        Ok(Outcome::Closed) => {}
        Err(StreamError::TimedOut(waiting_for)) => {
            counter!(METRIC_TIMED_OUT, "step" => waiting_for.as_str()).increment(1);
            let status = Status::deadline_exceeded(format!(
                "group creation stream timed out waiting for {}",
                waiting_for.as_str()
            ));
            let _ignore_closed_channel = outgoing.send(Err(status)).await;
        }
        Err(StreamError::Rejected(cause, status)) => {
            counter!(METRIC_REJECTED, "cause" => cause.as_str()).increment(1);
            debug!(cause = cause.as_str(), %status, "group creation stream rejected");
            let _ignore_closed_channel = outgoing.send(Err(status)).await;
        }
    }
}

async fn run_protocol_impl(
    backend: &impl CreateGroupStreamBackend,
    streams: &CreateGroupStreams,
    stop: &CancellationToken,
    mut incoming: impl Stream<Item = Result<CreateGroupStreamRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<CreateGroupStreamResponse, Status>>,
) -> Result<Outcome, StreamError> {
    let settings = backend.settings();
    let deadline = settings.createdeadline.to_std().unwrap_or_default();

    // As on the listen streams, the wait for the first step has no deadline.
    // The stream holds no slot and no allowance until Begin is authenticated.
    let Some(step) = next_step(&mut incoming, None, stop).await? else {
        return Ok(Outcome::Closed);
    };
    let create_group_stream_request::Step::Begin(begin) = step else {
        return Err(StreamError::protocol("expected begin step"));
    };
    // Create must arrive within the deadline of Begin, however long the
    // server's checks take.
    let create_deadline = Instant::now() + deadline;
    let (_slot, creator, reservation, reserved) =
        timeout_at(create_deadline, admit(backend, streams, begin))
            .await
            .map_err(|_elapsed| StreamError::TimedOut(WaitingFor::Admission))??;
    counter!(METRIC_BEGUN).increment(1);
    if !send_step(
        outgoing,
        create_group_stream_response::Step::Reserved(reserved),
    )
    .await
    {
        return Ok(Outcome::Closed);
    }

    let Some(step) = next_step(&mut incoming, Some(create_deadline), stop).await? else {
        return Ok(Outcome::Closed);
    };
    let create_group_stream_request::Step::Create(payload) = step else {
        return Err(StreamError::protocol("expected create step"));
    };
    backend
        .create_apq_group(&creator, &reservation, payload)
        .await
        .map_err(|status| StreamError::classify(status, RejectCause::InvalidCreate))?;

    // The group exists from here on, even if the client does not learn it.
    send_step(
        outgoing,
        create_group_stream_response::Step::Created(CreateGroupStreamCreated {}),
    )
    .await;
    Ok(Outcome::Created)
}

/// Authenticates the begin step, counts the stream against the user and the
/// creation allowance, and reserves fresh ids and the profile upload.
async fn admit(
    backend: &impl CreateGroupStreamBackend,
    streams: &CreateGroupStreams,
    begin: CreateGroupStreamBegin,
) -> Result<
    (
        StreamSlot,
        StreamCreator,
        StreamReservation,
        CreateGroupStreamReserved,
    ),
    StreamError,
> {
    let settings = backend.settings();
    let (creator, begin) = authenticate(backend, begin).await?;

    let slot = streams.acquire(
        &creator.user_id,
        settings.createstreams,
        settings.createstreamsperuser,
    )?;

    // Counts even if the stream fails later, so that failing at the create
    // step cannot be used to get ids and uploads for free.
    backend
        .consume_creation_allowance(&creator.user_id)
        .await
        .map_err(|status| StreamError::Rejected(RejectCause::RateLimited, status))?;

    let reservation = StreamReservation {
        t_group_id: backend.fresh_group_id(),
        pq_group_id: backend.fresh_group_id(),
    };
    let group_profile_provisioning = match begin.group_profile_size {
        Some(size) => backend
            .provision_group_profile(size)
            .await
            .map_err(|status| StreamError::classify(status, RejectCause::Protocol))?,
        None => None,
    };

    let reserved = CreateGroupStreamReserved {
        group_id: Some(reservation.t_group_id.ref_into()),
        pq_group_id: Some(reservation.pq_group_id.ref_into()),
        group_profile_provisioning,
    };
    Ok((slot, creator, reservation, reserved))
}

/// Checks the version and the signature of the begin step against the key the
/// AS stores for the claimed user.
async fn authenticate(
    backend: &impl CreateGroupStreamBackend,
    begin: CreateGroupStreamBegin,
) -> Result<(StreamCreator, CreateGroupStreamBeginPayload), StreamError> {
    let request = SignedRequest::from_nested(begin, |begin| begin.payload.as_ref())
        .ok_or_else(|| StreamError::protocol("missing begin payload"))?;
    let unverified = request.inner().payload.as_ref();
    backend
        .verify_client_version(unverified.and_then(|payload| payload.client_metadata.as_ref()))
        .map_err(|status| StreamError::Rejected(RejectCause::Version, status))?;
    let user_id: UserId = unverified
        .and_then(|payload| payload.user_id.clone())
        .ok_or_else(|| StreamError::protocol("missing user id"))?
        .try_into()
        .map_err(|_| StreamError::protocol("invalid user id"))?;

    let verifying_key = backend
        .user_verifying_key(&user_id)
        .await
        .map_err(StreamError::internal)?
        .ok_or_else(StreamError::unauthenticated)?;
    let payload: CreateGroupStreamBeginPayload = request
        .verify(&verifying_key)
        .map_err(|_| StreamError::unauthenticated())?;

    let creator = StreamCreator {
        user_id,
        verifying_key,
    };
    Ok((creator, payload))
}

/// Waits for the next step until the create `deadline`, if any, or until the
/// server stops.
async fn next_step(
    incoming: &mut (impl Stream<Item = Result<CreateGroupStreamRequest, Status>> + Unpin),
    deadline: Option<Instant>,
    stop: &CancellationToken,
) -> Result<Option<create_group_stream_request::Step>, StreamError> {
    let next = async {
        match deadline {
            Some(deadline) => timeout_at(deadline, incoming.next())
                .await
                .map_err(|_elapsed| StreamError::TimedOut(WaitingFor::Create)),
            None => Ok(incoming.next().await),
        }
    };
    let next = tokio::select! {
        _ = stop.cancelled() => {
            return Err(StreamError::Rejected(
                RejectCause::Shutdown,
                Status::unavailable("server stopped"),
            ));
        }
        next = next => next?,
    };
    match next {
        None => Ok(None),
        Some(Err(status)) => {
            debug!(%status, "group creation stream closed by the client");
            Ok(None)
        }
        Some(Ok(CreateGroupStreamRequest { step: None })) => {
            Err(StreamError::protocol("missing step"))
        }
        Some(Ok(CreateGroupStreamRequest { step: Some(step) })) => Ok(Some(step)),
    }
}

/// Returns false when the client is gone.
async fn send_step(
    outgoing: &mpsc::Sender<Result<CreateGroupStreamResponse, Status>>,
    step: create_group_stream_response::Step,
) -> bool {
    outgoing
        .send(Ok(CreateGroupStreamResponse { step: Some(step) }))
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use aircommon::{
        credentials::keys::ClientKeyType,
        crypto::signatures::{private_keys::SigningKey, signable::Signable},
        identifiers::Fqdn,
    };
    use airprotos::convert::TryRefInto;
    use tokio_stream::wrappers::ReceiverStream;
    use uuid::Uuid;

    use super::*;

    struct FakeBackend {
        settings: DsSettings,
        domain: Fqdn,
        reject_version: bool,
        lookup_fails: bool,
        lookup_delay: Duration,
        allowance_delay: Duration,
        keys: HashMap<UserId, ClientVerifyingKey>,
        allowance: AtomicUsize,
        consumed: AtomicUsize,
        create_status: Option<Status>,
        creates: Mutex<Vec<(UserId, StreamReservation)>>,
        stop: CancellationToken,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                settings: DsSettings::default(),
                domain: "example.com".parse().unwrap(),
                reject_version: false,
                lookup_fails: false,
                lookup_delay: Duration::ZERO,
                allowance_delay: Duration::ZERO,
                keys: HashMap::new(),
                allowance: AtomicUsize::new(100),
                consumed: AtomicUsize::new(0),
                create_status: None,
                creates: Mutex::new(Vec::new()),
                stop: CancellationToken::new(),
            }
        }

        fn register(&mut self) -> (UserId, SigningKey<ClientKeyType>) {
            let user_id = UserId::random(self.domain.clone());
            let signing_key = SigningKey::generate().unwrap();
            self.keys
                .insert(user_id.clone(), signing_key.verifying_key().clone());
            (user_id, signing_key)
        }

        fn consumed(&self) -> usize {
            self.consumed.load(Ordering::SeqCst)
        }

        fn record_create(&self, creator: &StreamCreator, reservation: StreamReservation) {
            self.creates
                .lock()
                .push((creator.user_id.clone(), reservation));
        }

        fn creates(&self) -> Vec<(UserId, StreamReservation)> {
            self.creates.lock().clone()
        }
    }

    impl CreateGroupStreamBackend for FakeBackend {
        fn settings(&self) -> &DsSettings {
            &self.settings
        }

        fn verify_client_version(
            &self,
            _client_metadata: Option<&ClientMetadata>,
        ) -> Result<(), Status> {
            if self.reject_version {
                return Err(Status::failed_precondition("unsupported version"));
            }
            Ok(())
        }

        async fn user_verifying_key(
            &self,
            user_id: &UserId,
        ) -> Result<Option<ClientVerifyingKey>, Status> {
            tokio::time::sleep(self.lookup_delay).await;
            if self.lookup_fails {
                return Err(Status::internal("lookup failed"));
            }
            Ok(self.keys.get(user_id).cloned())
        }

        async fn consume_creation_allowance(&self, _user_id: &UserId) -> Result<(), Status> {
            tokio::time::sleep(self.allowance_delay).await;
            self.allowance
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .map_err(|_| Status::resource_exhausted("Too many requests"))?;
            self.consumed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn fresh_group_id(&self) -> QualifiedGroupId {
            QualifiedGroupId::new(Uuid::new_v4(), self.domain.clone())
        }

        async fn provision_group_profile(
            &self,
            _size: i64,
        ) -> Result<Option<ProvisionAttachmentResponse>, Status> {
            Ok(Some(ProvisionAttachmentResponse::default()))
        }

        async fn create_apq_group(
            &self,
            creator: &StreamCreator,
            reservation: &StreamReservation,
            _payload: CreateApqGroupPayload,
        ) -> Result<(), Status> {
            self.record_create(creator, reservation.clone());
            self.create_status.clone().map_or(Ok(()), Err)
        }
    }

    fn begin(
        user_id: &UserId,
        signing_key: &SigningKey<ClientKeyType>,
    ) -> CreateGroupStreamRequest {
        let payload = CreateGroupStreamBeginPayload {
            client_metadata: None,
            user_id: Some(user_id.clone().into()),
            group_profile_size: Some(1024),
        };
        let begin = payload.sign(signing_key).unwrap();
        CreateGroupStreamRequest {
            step: Some(create_group_stream_request::Step::Begin(begin)),
        }
    }

    fn create() -> CreateGroupStreamRequest {
        CreateGroupStreamRequest {
            step: Some(create_group_stream_request::Step::Create(
                CreateApqGroupPayload::default(),
            )),
        }
    }

    struct Run {
        responses: Vec<Result<CreateGroupStreamResponse, Status>>,
    }

    impl Run {
        fn reserved(&self) -> Option<&CreateGroupStreamReserved> {
            self.responses.iter().find_map(|response| match response {
                Ok(CreateGroupStreamResponse {
                    step: Some(create_group_stream_response::Step::Reserved(reserved)),
                }) => Some(reserved),
                _ => None,
            })
        }

        fn created(&self) -> bool {
            self.responses.iter().any(|response| {
                matches!(
                    response,
                    Ok(CreateGroupStreamResponse {
                        step: Some(create_group_stream_response::Step::Created(_)),
                    })
                )
            })
        }

        fn error_code(&self) -> Option<Code> {
            self.responses
                .iter()
                .find_map(|response| response.as_ref().err().map(Status::code))
        }
    }

    /// Runs one stream on which the client sends `requests` and then either
    /// closes its side or keeps it open.
    async fn run_stream(
        backend: &FakeBackend,
        streams: &CreateGroupStreams,
        requests: Vec<CreateGroupStreamRequest>,
        keep_open: bool,
    ) -> Run {
        let (requests_tx, requests_rx) = mpsc::channel(requests.len().max(1));
        for request in requests {
            requests_tx.send(Ok(request)).await.unwrap();
        }
        let _keep_open = keep_open.then_some(requests_tx);
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel(4);
        run_protocol(
            backend,
            streams,
            &backend.stop,
            ReceiverStream::new(requests_rx),
            outgoing_tx,
        )
        .await;
        let mut responses = Vec::new();
        while let Some(response) = outgoing_rx.recv().await {
            responses.push(response);
        }
        Run { responses }
    }

    #[tokio::test]
    async fn creates_group_with_both_reserved_ids() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key), create()],
            false,
        )
        .await;

        let reserved = run.reserved().unwrap();
        assert!(reserved.group_profile_provisioning.is_some());
        let group_id: QualifiedGroupId =
            reserved.group_id.as_ref().unwrap().try_ref_into().unwrap();
        let pq_group_id: QualifiedGroupId = reserved
            .pq_group_id
            .as_ref()
            .unwrap()
            .try_ref_into()
            .unwrap();
        assert_ne!(group_id, pq_group_id);
        assert!(run.created());
        assert_eq!(run.error_code(), None);
        assert_eq!(backend.consumed(), 1);
        let creates = backend.creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].0, user_id);
        assert_eq!(creates[0].1.t_group_id, group_id);
        assert_eq!(creates[0].1.pq_group_id, pq_group_id);
        assert_eq!(streams.counts(&user_id), (0, 0));
    }

    #[tokio::test]
    async fn unknown_user_is_unauthenticated_and_consumes_nothing() {
        let backend = FakeBackend::new();
        let user_id = UserId::random(backend.domain.clone());
        let signing_key = SigningKey::generate().unwrap();
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::Unauthenticated));
        assert!(run.reserved().is_none());
        assert_eq!(backend.consumed(), 0);
    }

    #[tokio::test]
    async fn forged_begin_is_unauthenticated_and_consumes_nothing() {
        let mut backend = FakeBackend::new();
        let (victim, _victim_key) = backend.register();
        let forger_key = SigningKey::generate().unwrap();
        let streams = CreateGroupStreams::default();

        let run = run_stream(&backend, &streams, vec![begin(&victim, &forger_key)], false).await;

        assert_eq!(run.error_code(), Some(Code::Unauthenticated));
        assert!(run.reserved().is_none());
        assert_eq!(backend.consumed(), 0);
    }

    #[tokio::test]
    async fn key_lookup_failure_fails_closed() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.lookup_fails = true;
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::Internal));
        assert!(run.reserved().is_none());
        assert_eq!(backend.consumed(), 0);
    }

    #[tokio::test]
    async fn unsupported_version_is_rejected_before_authentication() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.reject_version = true;
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::FailedPrecondition));
        assert_eq!(backend.consumed(), 0);
    }

    #[tokio::test]
    async fn exhausted_allowance_gets_no_ids() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.allowance = AtomicUsize::new(0);
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::ResourceExhausted));
        assert!(run.reserved().is_none());
    }

    #[tokio::test]
    async fn begin_without_create_still_consumes_allowance() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert!(run.reserved().is_some());
        assert_eq!(run.error_code(), None);
        assert!(backend.creates().is_empty());
        assert_eq!(backend.consumed(), 1);
    }

    #[tokio::test]
    async fn failed_create_still_consumes_allowance() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.create_status = Some(Status::invalid_argument("invalid group"));
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key), create()],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::InvalidArgument));
        assert!(!run.created());
        assert_eq!(backend.consumed(), 1);
    }

    #[tokio::test]
    async fn create_before_begin_is_rejected() {
        let backend = FakeBackend::new();
        let streams = CreateGroupStreams::default();

        let run = run_stream(&backend, &streams, vec![create()], false).await;

        assert_eq!(run.error_code(), Some(Code::InvalidArgument));
        assert!(backend.creates().is_empty());
        assert_eq!(backend.consumed(), 0);
    }

    #[tokio::test]
    async fn second_begin_is_rejected() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key), begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert!(run.reserved().is_some());
        assert_eq!(run.error_code(), Some(Code::InvalidArgument));
        assert_eq!(backend.consumed(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn stop_ends_a_stream_waiting_for_begin() {
        let backend = FakeBackend::new();
        let streams = CreateGroupStreams::default();

        let stop = async {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            backend.stop.cancel();
        };
        let (run, ()) = tokio::join!(run_stream(&backend, &streams, Vec::new(), true), stop);

        assert_eq!(run.error_code(), Some(Code::Unavailable));
    }

    #[tokio::test(start_paused = true)]
    async fn stop_ends_a_stream_waiting_for_create() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();

        let stop = async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            backend.stop.cancel();
        };
        let (run, ()) = tokio::join!(
            run_stream(
                &backend,
                &streams,
                vec![begin(&user_id, &signing_key)],
                true,
            ),
            stop
        );

        assert!(run.reserved().is_some());
        assert_eq!(run.error_code(), Some(Code::Unavailable));
        assert_eq!(streams.counts(&user_id), (0, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn stream_without_create_times_out_after_the_deadline() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();

        let started = Instant::now();
        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            true,
        )
        .await;

        assert!(run.reserved().is_some());
        assert_eq!(run.error_code(), Some(Code::DeadlineExceeded));
        assert_eq!(started.elapsed(), Duration::from_secs(5 * 60));
        assert!(backend.creates().is_empty());
        assert_eq!(backend.consumed(), 1);
        assert_eq!(streams.counts(&user_id), (0, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_admission_times_out_and_releases_the_stream() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.allowance_delay = Duration::from_secs(60 * 60);
        let streams = CreateGroupStreams::default();

        let started = Instant::now();
        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            true,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::DeadlineExceeded));
        assert!(run.reserved().is_none());
        assert_eq!(started.elapsed(), Duration::from_secs(5 * 60));
        assert_eq!(streams.counts(&user_id), (0, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn create_deadline_counts_from_begin() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        backend.lookup_delay = Duration::from_secs(3 * 60);
        let streams = CreateGroupStreams::default();

        let started = Instant::now();
        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            true,
        )
        .await;

        assert!(run.reserved().is_some());
        assert_eq!(run.error_code(), Some(Code::DeadlineExceeded));
        assert_eq!(started.elapsed(), Duration::from_secs(5 * 60));
    }

    #[tokio::test]
    async fn user_at_the_stream_limit_is_rejected_without_consuming() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();
        let max_per_user = backend.settings.createstreamsperuser;
        let mut held = Vec::new();
        for _ in 0..max_per_user {
            let slot = streams
                .acquire(&user_id, backend.settings.createstreams, max_per_user)
                .unwrap();
            held.push(slot);
        }

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;

        assert_eq!(run.error_code(), Some(Code::ResourceExhausted));
        assert_eq!(backend.consumed(), 0);
        assert_eq!(streams.counts(&user_id), (max_per_user, max_per_user));

        held.pop();
        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;
        assert!(run.reserved().is_some());
    }

    #[tokio::test]
    async fn begin_at_the_process_limit_is_rejected_after_authentication() {
        let mut backend = FakeBackend::new();
        backend.settings.createstreams = 1;
        let (user_id, signing_key) = backend.register();
        let (other_user_id, _) = backend.register();
        let streams = CreateGroupStreams::default();
        let _held = streams.acquire(&other_user_id, 1, 1).unwrap();

        let forger_key = SigningKey::<ClientKeyType>::generate().unwrap();
        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &forger_key)],
            false,
        )
        .await;
        assert_eq!(run.error_code(), Some(Code::Unauthenticated));

        let run = run_stream(
            &backend,
            &streams,
            vec![begin(&user_id, &signing_key)],
            false,
        )
        .await;
        assert_eq!(run.error_code(), Some(Code::ResourceExhausted));
        assert_eq!(backend.consumed(), 0);
    }

    #[test]
    fn process_stream_limit_holds_and_releases() {
        let domain: Fqdn = "example.com".parse().unwrap();
        let streams = CreateGroupStreams::default();
        let first = streams
            .acquire(&UserId::random(domain.clone()), 2, 1)
            .unwrap();
        let _second = streams
            .acquire(&UserId::random(domain.clone()), 2, 1)
            .unwrap();
        assert!(
            streams
                .acquire(&UserId::random(domain.clone()), 2, 1)
                .is_err()
        );

        drop(first);
        assert!(streams.acquire(&UserId::random(domain), 2, 1).is_ok());
    }

    #[test]
    fn per_user_limit_counts_users_separately() {
        let domain: Fqdn = "example.com".parse().unwrap();
        let alice = UserId::random(domain.clone());
        let bob = UserId::random(domain);
        let streams = CreateGroupStreams::default();

        let alice_slot = streams.acquire(&alice, 10, 1).unwrap();
        assert!(streams.acquire(&alice, 10, 1).is_err());
        let _bob_slot = streams.acquire(&bob, 10, 1).unwrap();
        assert_eq!(streams.counts(&alice), (2, 1));

        drop(alice_slot);
        assert_eq!(streams.counts(&alice), (1, 0));
        assert!(!streams.open.lock().per_user.contains_key(&alice));
    }

    #[tokio::test]
    async fn concurrent_streams_of_one_user_hold_the_limit() {
        let mut backend = FakeBackend::new();
        let (user_id, signing_key) = backend.register();
        let streams = CreateGroupStreams::default();
        let max_per_user = backend.settings.createstreamsperuser;

        // Every stream reserves and then waits for its create step until the
        // client side is dropped.
        let mut clients = Vec::new();
        let mut protocols = Vec::new();
        for _ in 0..max_per_user + 3 {
            let (requests_tx, requests_rx) = mpsc::channel(1);
            requests_tx
                .send(Ok(begin(&user_id, &signing_key)))
                .await
                .unwrap();
            let (outgoing_tx, outgoing_rx) = mpsc::channel(4);
            clients.push((requests_tx, outgoing_rx));
            protocols.push(run_protocol(
                &backend,
                &streams,
                &backend.stop,
                ReceiverStream::new(requests_rx),
                outgoing_tx,
            ));
        }
        let protocols = futures_util::future::join_all(protocols);
        let check = async {
            let mut reserved = 0;
            let mut rejected = 0;
            for (_requests_tx, outgoing_rx) in &mut clients {
                match outgoing_rx.recv().await.unwrap() {
                    Ok(_) => reserved += 1,
                    Err(status) => {
                        assert_eq!(status.code(), Code::ResourceExhausted);
                        rejected += 1;
                    }
                }
            }
            assert_eq!(reserved, max_per_user);
            assert_eq!(rejected, 3);
            assert_eq!(backend.consumed(), max_per_user);
            clients.clear();
        };
        tokio::join!(protocols, check);
        assert_eq!(streams.counts(&user_id), (0, 0));
    }
}
