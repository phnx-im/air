// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group creation stream: authentication, id and credential binding, the
//! deadline and the stream caps.

use std::time::Duration;

use airapiclient::{
    ApiClient,
    ds_api::{DsGroupCreator, DsGroupReservation, DsRequestError},
};
use airbackend::settings::DsSettings;
use aircommon::{
    credentials::keys::{ClientKeyType, UserSigningKey},
    crypto::signatures::{private_keys::SigningKey, signable::Signable},
    identifiers::UserId,
};
use airprotos::delivery_service::v1::{
    CreateGroupStreamBeginPayload, CreateGroupStreamRequest, create_group_stream_request,
    delivery_service_client::DeliveryServiceClient,
};
use airserver_test_harness::utils::setup::{TestBackend, TestBackendParams};
use tonic::{Code, transport::Channel};

/// Every stream counts against the hourly creation allowance of its user.
const CREATION_ALLOWANCE: usize = 100;

#[track_caller]
fn assert_status(error: DsRequestError, code: Code, message: &str) {
    match error {
        DsRequestError::Tonic(status) => {
            assert_eq!(status.code(), code, "{status:?}");
            assert!(status.message().contains(message), "{status:?}");
        }
        error => panic!("expected a status, got {error:?}"),
    }
}

async fn begin(
    setup: &TestBackend,
    signing_key: &UserSigningKey,
) -> Result<(DsGroupReservation, DsGroupCreator), DsRequestError> {
    ApiClient::with_endpoint(&setup.server_url())
        .unwrap()
        .ds_begin_group_creation(signing_key, None)
        .await
}

/// Sends a begin step claiming `user_id`, signed with `signing_key`, and
/// returns the status the server closes the stream with.
async fn raw_begin_status(
    setup: &TestBackend,
    user_id: &UserId,
    signing_key: &SigningKey<ClientKeyType>,
) -> Code {
    let channel = Channel::from_shared(setup.server_url().to_string())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = DeliveryServiceClient::new(channel);
    let begin = CreateGroupStreamBeginPayload {
        client_metadata: None,
        user_id: Some(user_id.clone().into()),
        group_profile_size: None,
    }
    .sign(signing_key)
    .unwrap();
    let request = CreateGroupStreamRequest {
        step: Some(create_group_stream_request::Step::Begin(begin)),
    };
    let result = match client
        .create_group_stream(tokio_stream::iter([request]))
        .await
    {
        Ok(response) => response.into_inner().message().await.map(|_| ()),
        Err(status) => Err(status),
    };
    result.expect_err("expected the stream to fail").code()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stream_creates_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (reservation, creator) = ApiClient::with_endpoint(&setup.server_url())
        .unwrap()
        .ds_begin_group_creation(signing_key, Some(1024))
        .await
        .unwrap();
    assert_ne!(reservation.group_id, reservation.pq_group_id);
    assert!(reservation.group_profile_provisioning.is_some());

    let (params, ear_key) = alice
        .group_creation_params_for_test(reservation.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    creator.create(params, &ear_key).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stream_creates_self_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (reservation, creator) = begin(&setup, signing_key).await.unwrap();
    let (params, ear_key) = alice
        .self_group_creation_params_for_test(reservation.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    creator.create(params, &ear_key).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_with_another_users_credential_is_rejected() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (reservation, creator) = begin(&setup, bob.signing_key_for_test()).await.unwrap();
    let (params, ear_key) = alice
        .self_group_creation_params_for_test(reservation.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    let error = creator.create(params, &ear_key).await.unwrap_err();
    assert_status(
        error,
        Code::PermissionDenied,
        "creator credential does not belong",
    );
}

/// Opens and drops streams until the user's allowance is spent, and returns
/// how many got ids.
async fn spend_allowance(setup: &TestBackend, signing_key: &UserSigningKey) -> usize {
    for spent in 0..=CREATION_ALLOWANCE {
        match begin(setup, signing_key).await {
            Ok((_reservation, creator)) => drop(creator),
            Err(error) => {
                assert_status(error, Code::ResourceExhausted, "Too many requests");
                return spent;
            }
        }
    }
    panic!("allowance never ran out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn forged_and_unknown_begins_spend_nothing_and_real_begins_spend_one_unit() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, carol] = setup.add_users().await;
    let alice_key = alice.signing_key_for_test();
    let bob_key = bob.signing_key_for_test();
    let carol_key = carol.signing_key_for_test();

    let unknown_user = UserId::random(setup.domain().clone());
    assert_eq!(
        raw_begin_status(&setup, &unknown_user, bob_key).await,
        Code::Unauthenticated
    );

    // Bob claims to be Alice, with his own key
    for _ in 0..CREATION_ALLOWANCE {
        assert_eq!(
            raw_begin_status(&setup, alice.user_id(), bob_key).await,
            Code::Unauthenticated
        );
    }

    // Registration spends the same units for every user, so Alice has as many
    // left as Carol, who nobody impersonated. Each stream spends one unit even
    // though it never creates a group.
    let alice_streams = spend_allowance(&setup, alice_key).await;
    let carol_streams = spend_allowance(&setup, carol_key).await;
    assert_eq!(alice_streams, carol_streams);
    assert!(alice_streams > CREATION_ALLOWANCE - 10, "{alice_streams}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_with_ids_of_another_stream_is_rejected() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (first, first_creator) = begin(&setup, signing_key).await.unwrap();
    let (_second, second_creator) = begin(&setup, signing_key).await.unwrap();

    let (params, ear_key) = alice
        .group_creation_params_for_test(first.group_id.clone(), first.pq_group_id.clone())
        .await
        .unwrap();
    let error = second_creator.create(params, &ear_key).await.unwrap_err();
    assert_status(
        error,
        Code::InvalidArgument,
        "group id was not reserved by this stream",
    );

    // The ids stay with the stream that reserved them
    let (params, ear_key) = alice
        .group_creation_params_for_test(first.group_id, first.pq_group_id)
        .await
        .unwrap();
    first_creator.create(params, &ear_key).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_create_must_use_both_ids_of_its_stream() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (first, first_creator) = begin(&setup, signing_key).await.unwrap();
    let (second, _second_creator) = begin(&setup, signing_key).await.unwrap();

    let (params, ear_key) = alice
        .group_creation_params_for_test(first.group_id, second.pq_group_id)
        .await
        .unwrap();
    let error = first_creator.create(params, &ear_key).await.unwrap_err();
    assert_status(
        error,
        Code::InvalidArgument,
        "group id was not reserved by this stream",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_with_another_t_mls_group_id_is_rejected() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (reservation, creator) = begin(&setup, signing_key).await.unwrap();
    let (other, _other_creator) = begin(&setup, signing_key).await.unwrap();

    // The request names the reserved ids, but the T group is built for another
    let (mut params, ear_key) = alice
        .group_creation_params_for_test(other.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    params.group_id = reservation.group_id;
    let error = creator.create(params, &ear_key).await.unwrap_err();
    assert_status(error, Code::InvalidArgument, "MLS group id does not match");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_with_another_pq_mls_group_id_is_rejected() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (reservation, creator) = begin(&setup, signing_key).await.unwrap();
    let (other, _other_creator) = begin(&setup, signing_key).await.unwrap();

    // The request names the reserved ids, but the PQ group is built for another
    let (mut params, ear_key) = alice
        .group_creation_params_for_test(reservation.group_id, other.pq_group_id)
        .await
        .unwrap();
    params.pq.as_mut().unwrap().group_id = reservation.pq_group_id;
    let error = creator.create(params, &ear_key).await.unwrap_err();
    assert_status(error, Code::InvalidArgument, "MLS group id does not match");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_with_another_users_credential_is_rejected() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    // Bob authenticates, but the group carries Alice's credential.
    let (reservation, creator) = begin(&setup, bob.signing_key_for_test()).await.unwrap();
    let (params, ear_key) = alice
        .group_creation_params_for_test(reservation.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    let error = creator.create(params, &ear_key).await.unwrap_err();
    assert_status(
        error,
        Code::PermissionDenied,
        "creator credential does not belong",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stream_past_its_deadline_is_closed_and_its_ids_are_unusable() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        ds: DsSettings {
            createdeadline: chrono::Duration::seconds(1),
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;
    let signing_key = alice.signing_key_for_test();

    let (reservation, creator) = begin(&setup, signing_key).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    let (params, ear_key) = alice
        .group_creation_params_for_test(
            reservation.group_id.clone(),
            reservation.pq_group_id.clone(),
        )
        .await
        .unwrap();
    let error = creator.create(params, &ear_key).await.unwrap_err();
    assert_status(
        error,
        Code::DeadlineExceeded,
        "timed out waiting for create",
    );

    // The old endpoint does not accept stream ids either
    let (params, ear_key) = alice
        .group_creation_params_for_test(reservation.group_id, reservation.pq_group_id)
        .await
        .unwrap();
    let error = ApiClient::with_endpoint(&setup.server_url())
        .unwrap()
        .ds_create_group(params, signing_key, &ear_key)
        .await
        .unwrap_err();
    assert_status(error, Code::InvalidArgument, "unreserved group id");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn per_user_stream_limit_holds() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        ds: DsSettings {
            createstreamsperuser: 2,
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let [alice, bob] = setup.add_users().await;
    let alice_key = alice.signing_key_for_test();
    let bob_key = bob.signing_key_for_test();

    let first = begin(&setup, alice_key).await.unwrap();
    let _second = begin(&setup, alice_key).await.unwrap();
    let error = begin(&setup, alice_key).await.unwrap_err();
    assert_status(
        error,
        Code::ResourceExhausted,
        "Too many open group creation streams",
    );

    // Other users are not affected
    let _bob = begin(&setup, bob_key).await.unwrap();

    // A closed stream frees its slot once the server has seen it close
    drop(first);
    let mut reopened = None;
    for _ in 0..50 {
        if let Ok(stream) = begin(&setup, alice_key).await {
            reopened = Some(stream);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(reopened.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn process_stream_limit_holds() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        ds: DsSettings {
            createstreams: 1,
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let [alice, bob] = setup.add_users().await;
    let alice_key = alice.signing_key_for_test();
    let bob_key = bob.signing_key_for_test();

    let _alice = begin(&setup, alice_key).await.unwrap();
    let error = begin(&setup, bob_key).await.unwrap_err();
    assert_status(
        error,
        Code::ResourceExhausted,
        "Too many group creation streams",
    );
}
