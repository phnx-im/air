// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, sync::Arc};

use aircommon::{identifiers::Fqdn, time::Duration};
use sqlx::{PgPool, Postgres, pool::PoolConnection};
use tokio::sync::Mutex;
use tonic::Status;
use tracing::error;
use uuid::Uuid;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    ds::storage::Storage,
};
pub use grpc::GrpcDs;

mod attachments;
mod delete_group;
mod group_operation;
pub mod group_state;
pub mod grpc;
mod join_connection_group;
pub mod process;
mod resync;
mod self_remove;
pub mod storage;
mod update_user_profile_key;

/// Number of days after its last use upon which a group state is considered
/// expired.
pub const GROUP_STATE_EXPIRATION: Duration = Duration::days(90);

#[derive(Debug, Clone)]
pub struct Ds {
    own_domain: Fqdn,
    reserved_group_ids: Arc<Mutex<HashSet<Uuid>>>,
    db_pool: PgPool,
    storage: Option<Storage>,
}

#[derive(Debug)]
pub(crate) struct ReservedGroupId(Uuid);

impl BackendService for Ds {
    async fn initialize(db_pool: PgPool, domain: Fqdn) -> Result<Self, ServiceCreationError> {
        let ds = Self {
            own_domain: domain,
            reserved_group_ids: Default::default(),
            db_pool,
            storage: None,
        };

        Ok(ds)
    }
}

impl Ds {
    pub fn set_storage(&mut self, storage: Storage) {
        self.storage = Some(storage);
    }

    async fn reserve_group_id(&self, group_id: Uuid) -> bool {
        let mut reserved_group_ids = self.reserved_group_ids.lock().await;
        reserved_group_ids.insert(group_id)
    }

    async fn claim_reserved_group_id(&self, group_id: Uuid) -> Option<ReservedGroupId> {
        let mut reserved_group_ids = self.reserved_group_ids.lock().await;
        if reserved_group_ids.remove(&group_id) {
            Some(ReservedGroupId(group_id))
        } else {
            None
        }
    }

    fn own_domain(&self) -> &Fqdn {
        &self.own_domain
    }

    pub(crate) async fn connection(&self) -> Result<PoolConnection<Postgres>, Status> {
        self.db_pool.acquire().await.map_err(|error| {
            error!(%error, "Failed to acquire DB connection");
            Status::internal("Failed to acquire DB connection")
        })
    }
}
