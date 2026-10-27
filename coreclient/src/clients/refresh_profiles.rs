// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Re-fetching of all group and group member profiles.

use std::{collections::HashSet, time::Duration};

use airapiclient::{ClassifyRequestError, RequestErrorKind, ds_api::DsRequestError};
use aircommon::{
    credentials::LeafCredential,
    crypto::indexed_aead::keys::UserProfileKey,
    identifiers::{QualifiedGroupId, UserId},
    time::TimeStamp,
    utils::FibonacciBackoff,
};
use anyhow::Context;
use openmls::{group::GroupId, prelude::LeafNodeIndex};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::{
    Chat, ChatId,
    chats::GroupDataExt,
    clients::block_contact::BlockedContact,
    db::access::ReadConnection,
    groups::{Group, client_auth_info::StorableUserCredential},
    user_profiles::process::ExistingUserProfile,
};

use super::CoreUser;

/// Consecutive rate limited attempts before the member pass gives up.
const MAX_RATE_LIMITED_RETRIES: usize = 8;

impl CoreUser {
    /// Spawns a refresh of the profiles of all groups and group members.
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

    /// Schedules a fetch of the profile of every group and group member.
    ///
    /// Member profile keys are refreshed from the DS first.
    async fn refresh_all_profiles(&self) -> anyhow::Result<()> {
        info!("Refreshing all profiles");
        let groups = self.refresh_group_profiles().await?;
        info!(groups, "Scheduled group profile fetches");
        // Group fetches can run while the member keys are refreshed
        self.outbound_service().notify_profile_fetches();

        let MemberRefreshStats {
            outdated,
            up_to_date,
            skipped,
            failed_groups,
        } = self.refresh_member_profiles().await?;
        self.outbound_service().notify_profile_fetches();

        info!(
            outdated,
            up_to_date, skipped, failed_groups, "Refreshed member profile keys"
        );
        Ok(())
    }

    async fn refresh_group_profiles(&self) -> anyhow::Result<usize> {
        let own_user_id = self.user_id();
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<usize> {
                let group_ids = Group::load_all_group_ids(&mut *txn).await?;
                let chat_ids: HashSet<ChatId> = Chat::load_ordered_ids(&mut *txn)
                    .await?
                    .into_iter()
                    .collect();

                let mut scheduled = 0;
                for group_id in group_ids {
                    let Some((chat_id, group)) =
                        load_refreshable_group(&mut *txn, &group_id, &chat_ids).await?
                    else {
                        continue;
                    };

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

    /// Refreshes the profile keys of the members of all groups from the DS.
    ///
    /// A user in several groups is only checked in the first group that
    /// succeeds.
    async fn refresh_member_profiles(&self) -> anyhow::Result<MemberRefreshStats> {
        let (group_ids, chat_ids) = {
            let mut connection = self.db().read().await?;
            let group_ids = Group::load_all_group_ids(&mut connection).await?;
            let chat_ids: HashSet<ChatId> = Chat::load_ordered_ids(&mut connection)
                .await?
                .into_iter()
                .collect();
            (group_ids, chat_ids)
        };

        let mut stats = MemberRefreshStats::default();
        let mut seen = HashSet::new();
        let mut backoff = FibonacciBackoff::new();
        let mut rate_limited = 0;
        let total = group_ids.len();
        let mut group_ids = group_ids.into_iter().enumerate().peekable();
        while let Some((idx, group_id)) = group_ids.peek() {
            let progress = format!("{}/{total}", idx + 1);
            match self
                .refresh_group_member_profiles(group_id, &chat_ids, &mut seen, &mut stats)
                .await
            {
                Ok(()) => {}
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
                            "Rate limited while refreshing member profiles"
                        );
                        sleep(delay).await;
                        // Retry the same group
                        continue;
                    }
                    warn!(%progress, ?group_id, %error, "Failed to refresh member profiles");
                    stats.failed_groups += 1;
                }
            }
            backoff.reset();
            rate_limited = 0;
            group_ids.next();
        }
        Ok(stats)
    }

    async fn refresh_group_member_profiles(
        &self,
        group_id: &GroupId,
        chat_ids: &HashSet<ChatId>,
        seen: &mut HashSet<UserId>,
        stats: &mut MemberRefreshStats,
    ) -> anyhow::Result<()> {
        let mut connection = self.db().read().await?;
        let Some((_chat_id, group)) =
            load_refreshable_group(&mut connection, group_id, chat_ids).await?
        else {
            return Ok(());
        };

        let mut members: Vec<(LeafNodeIndex, StorableUserCredential)> = Vec::new();
        for member in group.mls_group().members() {
            let Ok(leaf_credential) = LeafCredential::from_credential(&member.credential) else {
                continue;
            };
            let user_id = leaf_credential.user_id(group.own_user_id());
            if user_id == self.user_id() || seen.contains(user_id) {
                continue;
            }
            if BlockedContact::check_blocked(&mut connection, user_id).await? {
                stats.skipped += 1;
                seen.insert(user_id.clone());
                continue;
            }
            let Some(credential) =
                StorableUserCredential::load_by_user_id(&mut connection, user_id).await?
            else {
                warn!(?user_id, "No credential for group member, skipping");
                stats.skipped += 1;
                continue;
            };
            members.push((member.index, credential));
        }
        drop(connection);
        if members.is_empty() {
            return Ok(());
        }

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

        let mut fetches = Vec::new();
        for (leaf_index, credential) in members {
            let user_id = credential.user_id().clone();
            let Some(eupk) = external_commit_info
                .indexed_encrypted_user_profile_keys
                .get(&leaf_index)
            else {
                warn!(
                    ?user_id,
                    "No user profile key on the DS for member, skipping"
                );
                stats.skipped += 1;
                continue;
            };
            match UserProfileKey::decrypt(group.identity_link_wrapper_key(), eupk, &user_id) {
                Ok(user_profile_key) => {
                    fetches.push((credential, user_profile_key));
                    seen.insert(user_id);
                }
                Err(error) => {
                    warn!(?user_id, %error, "Failed to decrypt member profile key");
                    stats.skipped += 1;
                }
            }
        }

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                for (credential, user_profile_key) in fetches {
                    let user_id = credential.user_id();
                    let existing = ExistingUserProfile::load(&mut *txn, user_id).await?;
                    if existing.matches_index(user_profile_key.index()) {
                        stats.up_to_date += 1;
                        continue;
                    }
                    info!(?user_id, "Member profile key is outdated, scheduling fetch");
                    Self::schedule_fetch_user_profile(
                        &mut *txn,
                        (credential.into(), user_profile_key),
                    )
                    .await?;
                    stats.outdated += 1;
                }
                Ok(())
            })
            .await
    }
}

/// Loads the group if it belongs to a non-blocked chat and is active and not
/// the self group.
async fn load_refreshable_group(
    connection: &mut impl ReadConnection,
    group_id: &GroupId,
    chat_ids: &HashSet<ChatId>,
) -> anyhow::Result<Option<(ChatId, Group)>> {
    let chat_id = ChatId::try_from(group_id).context("invalid group id")?;
    if !chat_ids.contains(&chat_id) {
        return Ok(None);
    }
    if BlockedContact::check_blocked_chat(&mut *connection, chat_id).await? {
        return Ok(None);
    }
    let Some(group) = Group::load(&mut *connection, group_id).await? else {
        return Ok(None);
    };
    if group.is_self_group() || !group.mls_group().is_active() {
        return Ok(None);
    }
    Ok(Some((chat_id, group)))
}

#[derive(Default)]
struct MemberRefreshStats {
    /// Stored profile key is outdated, a fetch was scheduled.
    outdated: usize,
    up_to_date: usize,
    skipped: usize,
    failed_groups: usize,
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
