// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Re-fetching of all group and contact profiles.

use std::time::Duration;

use airapiclient::{ClassifyRequestError, RequestErrorKind, ds_api::DsRequestError};
use aircommon::{
    credentials::LeafCredential, crypto::indexed_aead::keys::UserProfileKey,
    identifiers::QualifiedGroupId, time::TimeStamp, utils::FibonacciBackoff,
};
use anyhow::Context;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::{
    Chat, ChatId,
    chats::GroupDataExt,
    clients::block_contact::BlockedContact,
    contacts::Contact,
    groups::{Group, client_auth_info::StorableUserCredential},
    user_profiles::process::ExistingUserProfile,
};

use super::CoreUser;

/// Consecutive rate limited attempts before the contact pass gives up.
const MAX_RATE_LIMITED_RETRIES: usize = 8;

impl CoreUser {
    /// Spawns a refresh of the profiles of all groups and contacts.
    pub fn spawn_refresh_all_profiles(&self) {
        let user = self.clone();
        let cancel = self.inner.event_loop_cancel.token().clone();
        tokio::spawn(async move {
            let result = tokio::select! {
                result = user.refresh_all_profiles() => result,
                _ = cancel.cancelled() => {
                    debug!("user closed, stopping the profile refresh");
                    return;
                }
            };
            if let Err(error) = result {
                error!(%error, "Failed to refresh profiles");
            }
        });
    }

    /// Schedules a fetch of the profile of every group and contact.
    ///
    /// Contact profile keys are refreshed from the DS first.
    async fn refresh_all_profiles(&self) -> anyhow::Result<()> {
        info!("Refreshing all profiles");
        let groups = self.refresh_group_profiles().await?;
        info!(groups, "Scheduled group profile fetches");
        // Group fetches can run while the contact keys are refreshed
        self.outbound_service().notify_profile_fetches();

        let ContactRefreshStats {
            outdated,
            up_to_date,
            skipped,
            failed,
        } = self.refresh_contact_profiles().await?;
        self.outbound_service().notify_profile_fetches();

        info!(
            outdated,
            up_to_date, skipped, failed, "Refreshed contact profile keys"
        );
        Ok(())
    }

    async fn refresh_group_profiles(&self) -> anyhow::Result<usize> {
        let own_user_id = self.user_id();
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<usize> {
                let group_ids = Group::load_all_group_ids(&mut *txn).await?;
                let mut chat_ids = Chat::load_ordered_ids(&mut *txn).await?;
                chat_ids.sort_unstable();

                let mut scheduled = 0;
                for group_id in group_ids {
                    let chat_id = ChatId::try_from(&group_id).context("invalid group id")?;
                    if chat_ids.binary_search(&chat_id).is_err() {
                        continue;
                    }
                    if BlockedContact::check_blocked_chat(&mut *txn, chat_id).await? {
                        continue;
                    }
                    let Some(group) = Group::load(&mut *txn, &group_id).await? else {
                        continue;
                    };
                    if group.is_self_group() || !group.mls_group().is_active() {
                        continue;
                    }

                    let group_data = match group.group_data() {
                        Ok(group_data) => group_data,
                        Err(error) => {
                            warn!(%chat_id, %error, "Failed to decode group data");
                            continue;
                        }
                    };
                    let Some(group_data) = group_data else {
                        continue;
                    };
                    let (_title, external_group_profile) =
                        group_data.into_parts(group.identity_link_wrapper_key());
                    let Some(external_group_profile) = external_group_profile else {
                        continue;
                    };

                    Self::schedule_fetch_group_profile(
                        &mut *txn,
                        group_id,
                        own_user_id.clone(),
                        TimeStamp::now(),
                        external_group_profile,
                        true,
                    )
                    .await?;
                    scheduled += 1;
                }
                Ok(scheduled)
            })
            .await
    }

    async fn refresh_contact_profiles(&self) -> anyhow::Result<ContactRefreshStats> {
        let mut stats = ContactRefreshStats::default();
        let mut backoff = FibonacciBackoff::new();
        let mut rate_limited = 0;
        let contacts = self.contacts().await?;
        let total = contacts.len();
        let mut contacts = contacts.into_iter().enumerate().peekable();
        while let Some((idx, contact)) = contacts.peek() {
            let user_id = &contact.user_id;
            let progress = format!("{}/{total}", idx + 1);
            match self.refresh_contact_profile(contact).await {
                Ok(outcome) => {
                    info!(%progress, ?user_id, ?outcome, "Refreshed contact profile key");
                    match outcome {
                        ContactRefresh::Outdated => stats.outdated += 1,
                        ContactRefresh::UpToDate => stats.up_to_date += 1,
                        ContactRefresh::Skipped => stats.skipped += 1,
                    }
                }
                Err(error) => {
                    if let Some(retry_after) = rate_limited_retry_after(&error) {
                        rate_limited += 1;
                        if rate_limited > MAX_RATE_LIMITED_RETRIES {
                            return Err(error.context("Rate limited, giving up"));
                        }
                        let delay = backoff.next_backoff().max(retry_after.unwrap_or_default());
                        warn!(
                            %progress,
                            ?delay,
                            attempt = rate_limited,
                            "Rate limited while refreshing contact profiles"
                        );
                        sleep(delay).await;
                        // Retry the same contact
                        continue;
                    }
                    warn!(%progress, ?user_id, %error, "Failed to refresh contact profile");
                    stats.failed += 1;
                }
            }
            backoff.reset();
            rate_limited = 0;
            contacts.next();
        }
        Ok(stats)
    }

    async fn refresh_contact_profile(&self, contact: &Contact) -> anyhow::Result<ContactRefresh> {
        let Contact {
            user_id, chat_id, ..
        } = contact;

        let mut connection = self.db().read().await?;
        if BlockedContact::check_blocked(&mut connection, user_id).await? {
            return Ok(ContactRefresh::Skipped);
        }
        let group = Group::load_with_chat_id(&mut connection, *chat_id)
            .await?
            .context("connection group not found")?;
        let Some(credential) =
            StorableUserCredential::load_by_user_id(&mut connection, user_id).await?
        else {
            warn!(?user_id, "No credential for contact; skipping");
            return Ok(ContactRefresh::Skipped);
        };
        drop(connection);

        let leaf_index = group.mls_group().members().find_map(|member| {
            let leaf_credential = LeafCredential::from_credential(&member.credential).ok()?;
            (leaf_credential.user_id(group.own_user_id()) == user_id).then_some(member.index)
        });
        let leaf_index = leaf_index.context("contact is not a member of the connection group")?;

        let qgid: QualifiedGroupId = group.group_id().try_into()?;
        let external_commit_info = self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_external_commit_info(
                group.group_id().clone(),
                group.pq_group_id(),
                group.group_state_ear_key(),
            )
            .await?;

        let Some(eupk) = external_commit_info
            .indexed_encrypted_user_profile_keys
            .get(&leaf_index)
        else {
            warn!(
                ?user_id,
                "No user profile key on the DS for contact; skipping"
            );
            return Ok(ContactRefresh::Skipped);
        };
        let user_profile_key =
            UserProfileKey::decrypt(group.identity_link_wrapper_key(), eupk, user_id)?;

        let outcome = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let existing = ExistingUserProfile::load(&mut *txn, user_id).await?;
                if existing.matches_index(user_profile_key.index()) {
                    return Ok(ContactRefresh::UpToDate);
                }
                Self::schedule_fetch_user_profile(&mut *txn, (credential.into(), user_profile_key))
                    .await?;
                Ok(ContactRefresh::Outdated)
            })
            .await?;
        Ok(outcome)
    }
}

#[derive(Debug)]
enum ContactRefresh {
    /// The stored profile key is outdated, a fetch was scheduled.
    Outdated,
    UpToDate,
    Skipped,
}

#[derive(Default)]
struct ContactRefreshStats {
    outdated: usize,
    up_to_date: usize,
    skipped: usize,
    failed: usize,
}

/// The server's retry-after, if `error` is a rate limited DS request.
fn rate_limited_retry_after(error: &anyhow::Error) -> Option<Option<Duration>> {
    error.chain().find_map(
        |error| match error.downcast_ref::<DsRequestError>()?.kind() {
            RequestErrorKind::RateLimited { retry_after } => Some(retry_after),
            _ => None,
        },
    )
}
