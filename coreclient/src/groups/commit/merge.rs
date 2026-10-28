// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{
    credentials::{GroupStorageWitness, LeafCredential},
    time::TimeStamp,
    utils::removed_client,
};
use airprotos::client::{app_data::GroupAppData, group::GroupData};
use anyhow::{Context, Result, bail};
use mimi_room_policy::RoleIndex;
use openmls::prelude::{OpenMlsProvider, ProposalType, Sender, StagedCommit};
use tracing::{Level, debug, enabled};

use crate::{
    ChatId, SystemMessage,
    chats::messages::TimestampedMessage,
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    pub(crate) async fn discard_pending_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> Result<()> {
        self.clear_commit_failed(&mut *txn).await?;
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        self.pending_diff = None;
        // Clear the pending commit in the T group...
        self.mls_group.clear_pending_commit(provider.storage())?;
        // ... and in the PQ group if it exists.
        if let Some(pq) = &mut self.pq {
            pq.mls_group.clear_pending_commit(provider.storage())?;
        }
        Ok(())
    }

    /// Applies the staged operations of the given `StagedCommit` to the room
    /// state of this group. If no `StagedCommit` is given, apply the operation
    /// of the pending commit of this group, if any.
    fn apply_staged_operations_to_room_state(
        &mut self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Result<()> {
        for (remover, removed) in self.staged_commit_removes(staged_commit) {
            self.room_state_change_role_identity(&remover, &removed, RoleIndex::Outsider)?;
        }
        for (adder, added) in self.pending_adds(staged_commit) {
            self.room_state_change_role_identity(&adder, &added, RoleIndex::Regular)?;
        }

        Ok(())
    }

    /// If a [`StagedCommit`] is given, merge it and apply the pending group
    /// diff. If no [`StagedCommit`] is given, merge any pending commit and
    /// apply the pending group diff.
    ///
    /// Returns the messages resulting from the commit and any group data
    /// extracted from the staged commit.
    pub(in crate::groups) async fn merge_pending_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        verified: &impl GroupStorageWitness,
        staged_commit_option: impl Into<Option<StagedCommit>>,
        ds_timestamp: TimeStamp,
    ) -> Result<(Vec<TimestampedMessage>, Option<GroupData>)> {
        let staged_commit_option: Option<StagedCommit> = staged_commit_option.into();
        let provider = AirOpenMlsProvider::new(txn.as_mut());

        self.apply_staged_operations_to_room_state(staged_commit_option.as_ref())?;

        let (event_messages, group_data) = if let Some(staged_commit) = staged_commit_option {
            // Compute the messages we want to emit from the staged commit and the
            // client info diff.
            let staged_commit_messages = TimestampedMessage::from_staged_commit(
                self,
                verified,
                &staged_commit,
                ds_timestamp,
            )?;

            let group_data = self.group_data_from_staged_commit(&staged_commit)?;

            self.mls_group
                .merge_staged_commit(&provider, staged_commit)?;
            if let Some(pq) = &mut self.pq
                && pq.mls_group.pending_commit().is_some()
            {
                pq.mls_group.merge_pending_commit(&provider)?;
            }
            (staged_commit_messages, group_data)
        } else {
            // If we're merging a pending commit, we need to check if we have
            // committed a remove proposal by reference. If we have, we need to
            // create a notification message.
            let (staged_commit_messages, group_data) =
                if let Some(staged_commit) = self.mls_group.pending_commit() {
                    let group_data = self.group_data_from_staged_commit(staged_commit)?;
                    let messages = TimestampedMessage::from_staged_commit(
                        self,
                        verified,
                        staged_commit,
                        ds_timestamp,
                    )?;
                    (messages, group_data)
                } else {
                    (vec![], None)
                };

            self.mls_group.merge_pending_commit(&provider)?;
            if let Some(pq) = &mut self.pq
                && pq.mls_group.pending_commit().is_some()
            {
                pq.mls_group.merge_pending_commit(&provider)?;
            }
            (staged_commit_messages, group_data)
        };

        // We now apply the diff (if present)
        if let Some(diff) = self.pending_diff.take() {
            if let Some(identity_link_wrapper_key) = diff.identity_link_wrapper_key {
                self.identity_link_wrapper_key = identity_link_wrapper_key;
            }
            if let Some(group_state_ear_key) = diff.group_state_ear_key {
                self.group_state_ear_key = group_state_ear_key;
            }
        }

        self.pending_diff = None;
        self.send_message_collision_key = None;
        self.clear_commit_failed(&mut *txn).await?;

        // The linked-device list joins metadata with the live self-group
        // members. Notify its self-chat listener whenever a commit changes the
        // locally stored self group.
        if self.is_self_group()
            && let Some(chat_id) = ChatId::load_from_group_id(&mut *txn, self.group_id()).await?
        {
            txn.notifier().update(chat_id);
        }

        Ok((event_messages, group_data))
    }
}

impl TimestampedMessage {
    /// Turn a staged commit into a list of messages based on the proposals it
    /// includes.
    fn from_staged_commit(
        group: &Group,
        verified: &impl GroupStorageWitness,
        staged_commit: &StagedCommit,
        ds_timestamp: TimeStamp,
    ) -> Result<Vec<Self>> {
        // A commit that sets the tombstone closes the group. Its removes are
        // only there for legacy clients.
        if GroupAppData::has_tombstone(staged_commit.group_context().extensions())
            && !GroupAppData::has_tombstone(group.mls_group().extensions())
        {
            let leaf_node = staged_commit
                .update_path_leaf_node()
                .context("Delete commit without update path")?;
            let deleter = LeafCredential::from_credential(leaf_node.credential())?
                .user_id(group.own_user_id())
                .clone();
            return Ok(vec![TimestampedMessage::system_message(
                SystemMessage::DeleteGroup(deleter),
                ds_timestamp,
            )]);
        }

        // Collect the remover/removed pairs into a set to avoid duplicates.
        let mut removed_set = HashSet::new();
        let mut devices_unlinked_set = HashSet::new();
        let remove_proposals = staged_commit.queued_proposals().filter(|&p| {
            matches!(
                p.proposal().proposal_type(),
                ProposalType::Remove | ProposalType::SelfRemove
            )
        });
        for remove_proposal in remove_proposals {
            let sender_index = match remove_proposal.sender() {
                Sender::Member(leaf_node_index) => leaf_node_index,
                Sender::External(_) | Sender::NewMemberProposal => {
                    bail!("Only member proposals are supported for now")
                }
                Sender::NewMemberCommit => {
                    // This can only happen if the removed member is rejoining
                    // as part of the commit. No need to create a message.
                    continue;
                }
            };

            let remover = group
                .user_id_at(*sender_index, verified)?
                .context("Could not find user credential of message sender")?;

            let Some(removed_index) = removed_client(remove_proposal) else {
                // This cannot happen since we filtered for remove proposals.
                continue;
            };

            let removed_credential = group
                .unverified_credential_at(removed_index)?
                .context("Could not find credential of removed")?;

            match &removed_credential {
                LeafCredential::SelfGroup(self_group_credential) => {
                    devices_unlinked_set.insert(self_group_credential.client_id());
                }
                LeafCredential::User(_) => {
                    let removed = removed_credential.user_id(group.own_user_id()).clone();
                    if remover == removed {
                        // A system message for this proposal was already made when it was proposed
                        continue;
                    }
                    removed_set.insert((remover, removed));
                }
            }
        }
        let remove_messages = removed_set.into_iter().map(|(remover, removed)| {
            TimestampedMessage::system_message(
                SystemMessage::Remove(Some(remover), removed),
                ds_timestamp,
            )
        });
        let device_unlinked_messages = devices_unlinked_set.into_iter().map(|client_id| {
            TimestampedMessage::system_message(
                SystemMessage::DeviceUnlinked(client_id),
                ds_timestamp,
            )
        });

        // Collect adder and addee names and filter out duplicates
        let mut adds_set = HashSet::new();
        let mut devices_linked_set = HashSet::new();
        for staged_add_proposal in staged_commit.add_proposals() {
            let Sender::Member(sender_index) = staged_add_proposal.sender() else {
                // We don't support non-member adds.
                bail!("Non-member add proposal")
            };
            // Get the user id of the sender from the MLS group member credential
            let sender_id = group
                .user_id_at(*sender_index, verified)?
                .context("Could not find user credential of sender")?;

            // Get the credential of the added member from the proposal key package
            let credential = staged_add_proposal
                .add_proposal()
                .key_package()
                .leaf_node()
                .credential();
            let credential = LeafCredential::from_credential(credential)?;

            match &credential {
                LeafCredential::SelfGroup(self_group_credential) => {
                    devices_linked_set.insert(self_group_credential.client_id());
                }
                LeafCredential::User(_) => {
                    let addee_id = credential.user_id(group.own_user_id()).clone();
                    adds_set.insert((sender_id, addee_id));
                }
            }
        }
        let add_messages = adds_set.into_iter().map(|(adder, added)| {
            TimestampedMessage::system_message(SystemMessage::Add(Some(adder), added), ds_timestamp)
        });
        let device_linked_messages = devices_linked_set.into_iter().map(|client_id| {
            TimestampedMessage::system_message(SystemMessage::DeviceLinked(client_id), ds_timestamp)
        });

        let event_messages = remove_messages
            .chain(device_unlinked_messages)
            .chain(add_messages)
            .chain(device_linked_messages)
            .collect();

        // Emit log messages for updates.
        for staged_update_proposal in staged_commit.update_proposals() {
            let Sender::Member(sender_index) = staged_update_proposal.sender() else {
                // Update proposals have to be sent by group members.
                bail!("Invalid proposal")
            };
            if enabled!(Level::DEBUG) {
                let user_id = group
                    .user_id_at(*sender_index, verified)?
                    .context("Could not find user credential of sender")?;
                debug!(
                    ?user_id,
                    %sender_index, "Client has updated their key material",
                );
            }
        }

        Ok(event_messages)
    }
}
