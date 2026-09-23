// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::{
    identifiers::{Fqdn, QualifiedGroupId},
    time::Duration,
};
use parking_lot::Mutex;
use sqlx::PgPool;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    ds::{
        group_id_reservations::{
            GroupIdReservations, GroupIdReservationsFull, sweep_expired_group_id_reservations,
        },
        storage::Storage,
    },
    version::VersionPolicy,
};
pub use grpc::GrpcDs;

mod apq;
mod attachments;
mod collision_tags;
mod create_group;
mod delete_group;
mod epoch_snapshot;
mod group_id_reservations;
mod group_operation;
pub mod group_state;
pub mod grpc;
mod join_connection_group;
pub mod process;
mod resync;
mod self_remove;
pub mod storage;
mod update_user_profile_key;
mod welcome_info;

/// Number of days after its last use upon which a group state is considered
/// expired.
pub const GROUP_STATE_EXPIRATION: Duration = Duration::days(90);

/// How long the welcome information of an epoch is kept.
pub const WELCOME_INFO_EXPIRATION: Duration = Duration::days(90);

/// How long the snapshot of an epoch is kept. A sibling emulator client that
/// comes back later falls back to a resync at the current epoch.
pub const EPOCH_SNAPSHOT_EXPIRATION: Duration = Duration::days(90);

#[derive(Debug, Clone)]
pub struct Ds {
    own_domain: Fqdn,
    group_id_reservations: Arc<Mutex<GroupIdReservations>>,
    db_pool: PgPool,
    storage: Option<Storage>,
    version_policy: VersionPolicy,
}

#[derive(Debug)]
pub(crate) struct ReservedGroupId(Uuid);

impl BackendService for Ds {
    async fn initialize(
        db_pool: PgPool,
        domain: Fqdn,
        version_policy: VersionPolicy,
        stop: CancellationToken,
    ) -> Result<Self, ServiceCreationError> {
        let group_id_reservations = Arc::new(Mutex::new(GroupIdReservations::default()));
        tokio::spawn(
            stop.run_until_cancelled_owned(sweep_expired_group_id_reservations(
                group_id_reservations.clone(),
            )),
        );

        let ds = Self {
            own_domain: domain,
            group_id_reservations,
            db_pool,
            storage: None,
            version_policy,
        };

        Ok(ds)
    }

    fn describe_metrics() {
        GroupIdReservations::describe_metrics();
    }
}

impl Ds {
    pub fn set_storage(&mut self, storage: Storage) {
        self.storage = Some(storage);
    }

    /// Reserves a fresh group id, and a second one for the PQ leg of an APQ
    /// group when requested. Either every requested id is reserved or none.
    /// Reservations are released again unless group creation claims them
    /// within the TTL.
    pub(crate) fn request_group_ids(
        &self,
        with_pq_group_id: bool,
    ) -> Result<(QualifiedGroupId, Option<QualifiedGroupId>), GroupIdReservationsFull> {
        let now = Instant::now();
        let qualify = |group_uuid| QualifiedGroupId::new(group_uuid, self.own_domain.clone());
        if with_pq_group_id {
            let [group_uuid, pq_group_uuid] =
                self.group_id_reservations.lock().reserve_fresh::<2>(now)?;
            Ok((qualify(group_uuid), Some(qualify(pq_group_uuid))))
        } else {
            let [group_uuid] = self.group_id_reservations.lock().reserve_fresh::<1>(now)?;
            Ok((qualify(group_uuid), None))
        }
    }

    fn claim_reserved_group_id(&self, group_id: Uuid) -> Option<ReservedGroupId> {
        self.group_id_reservations
            .lock()
            .claim(group_id, Instant::now())
    }

    fn own_domain(&self) -> &Fqdn {
        &self.own_domain
    }
}
