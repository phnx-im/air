// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Builds the topology the walk operates on: `fleet.members[0]` (the hub)
//! connects to every other member and owns the group, and every other member
//! additionally connects to a handful of peers so it holds contacts of its own.
//!
//! The mesh is what lets a member other than the hub drive an invite: adds are
//! contact-gated, so a client can only add someone it is already connected to.
//! Without it the hub would be the sole committer of adds.
//!
//! Every step checks current local state first, so re-running against an
//! already-bootstrapped fleet only does the work that is still missing.

use std::collections::HashSet;

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, UsernameRecord, clients::CoreUser};
use indicatif::{MultiProgress, ProgressBar};
use tracing::info;

use crate::{fleet::Fleet, ops, parallel, progress::bar_style, report::Report};

pub struct BootstrapParams<'a> {
    pub chat_title: &'a str,
    pub invite_batch_size: usize,
    /// Peers each non-hub member connects to on top of the hub. Zero leaves
    /// the fleet a pure star, so only the hub can ever invite.
    pub contact_mesh_degree: usize,
    /// Whether to finish by having every member drain and commit a key
    /// update, so the walk starts from a fully converged, freshly rekeyed
    /// group. Costs one commit per member plus the fan-out to process them.
    pub full_update: bool,
    /// How many per-member operations may run at once.
    pub concurrency: usize,
}

pub async fn run(
    fleet: &Fleet,
    params: &BootstrapParams<'_>,
    report: &mut Report,
    multi: &MultiProgress,
) -> anyhow::Result<ChatId> {
    let hub = &fleet.members[0].user;

    // Usernames first, for the whole fleet: the mesh phase needs a record for
    // any member it might connect to, not just the ones the hub still has to
    // reach. Already-registered members just load their existing record. One
    // member per task, since each only touches its own store.
    let username_bar = multi.add(ProgressBar::new(fleet.members.len() as u64));
    username_bar.set_style(bar_style("registering usernames"));
    let username_results = parallel::map(
        fleet.members.iter().map(|m| m.user.clone()).collect(),
        params.concurrency,
        {
            let bar = username_bar.clone();
            move |user| {
                let bar = bar.clone();
                async move {
                    let result = ops::ensure_username(&user).await;
                    bar.inc(1);
                    result
                }
            }
        },
    )
    .await;
    let mut records: Vec<Option<UsernameRecord>> = Vec::with_capacity(fleet.members.len());
    for result in username_results {
        report.record("ensure_username", &result);
        records.push(result.ok());
    }
    username_bar.finish_with_message("usernames registered");

    connect_to_hub(fleet, &records, report, multi).await?;
    build_contact_mesh(fleet, &records, params, report, multi).await?;

    let chat_id = match find_group_chat(hub, params.chat_title).await? {
        Some(chat_id) => chat_id,
        None => {
            let chat_id = ops::create_group(hub, params.chat_title.to_owned()).await?;
            info!(%chat_id, "created group");
            chat_id
        }
    };

    invite_everyone(fleet, chat_id, params.invite_batch_size, report, multi).await?;

    if params.full_update {
        full_update_sweep(fleet, chat_id, params.concurrency, report, multi).await;
    }

    Ok(chat_id)
}

/// Connects the hub to every other member, so it holds the full contact list
/// and can always invite.
///
/// Sequential, and unavoidably so: the hub is one endpoint of every edge, and
/// `connect` drains the initiator. Overlapping these would mean concurrent
/// drains on the hub, which would corrupt its queue ratchet.
async fn connect_to_hub(
    fleet: &Fleet,
    records: &[Option<UsernameRecord>],
    report: &mut Report,
    multi: &MultiProgress,
) -> anyhow::Result<()> {
    let hub = &fleet.members[0].user;
    let hub_contacts = contact_ids(hub).await?;

    let bar = multi.add(ProgressBar::new(
        fleet.members.len().saturating_sub(1) as u64
    ));
    bar.set_style(bar_style("connecting clients"));

    for (index, member) in fleet.members.iter().enumerate().skip(1) {
        let client: &CoreUser = &member.user;
        if hub_contacts.contains(client.user_id()) {
            bar.inc(1);
            continue;
        }
        let Some(record) = &records[index] else {
            bar.inc(1);
            continue;
        };

        let connect_result = ops::connect(hub, client, record).await;
        report.record("connect", &connect_result);
        if let Ok(chat_id) = &connect_result {
            info!(index = member.index, %chat_id, "connected client to hub");
        }
        bar.inc(1);
    }
    bar.finish_with_message("clients connected");
    Ok(())
}

/// Connects each non-hub member to the `degree` members that follow it in
/// index order, wrapping around. A deterministic ring keeps the mesh stable
/// across resumed runs, and gives every member roughly `2 * degree` contacts
/// once incoming edges are counted.
///
/// Built one offset at a time. Within a single offset the edges `i -> i+offset`
/// form a permutation of the clients, so every member is the initiator of
/// exactly one edge and the responder of exactly one other. That is what makes
/// the round safe to fan out: no member is ever drained by two tasks at once.
async fn build_contact_mesh(
    fleet: &Fleet,
    records: &[Option<UsernameRecord>],
    params: &BootstrapParams<'_>,
    report: &mut Report,
    multi: &MultiProgress,
) -> anyhow::Result<()> {
    let client_count = fleet.members.len().saturating_sub(1);
    // Each client can reach at most every other client.
    let degree = params
        .contact_mesh_degree
        .min(client_count.saturating_sub(1));
    if degree == 0 {
        return Ok(());
    }

    let bar = multi.add(ProgressBar::new((degree * client_count) as u64));
    bar.set_style(bar_style("building contact mesh"));

    for offset in 1..=degree {
        let mut edges = Vec::with_capacity(client_count);
        for index in 1..fleet.members.len() {
            // Walk the ring over indices 1..len, skipping the hub at 0.
            let peer = 1 + ((index - 1 + offset) % client_count);
            if peer == index {
                continue;
            }
            let Some(record) = records[peer].clone() else {
                continue;
            };
            edges.push((
                fleet.members[index].user.clone(),
                fleet.members[peer].user.clone(),
                record,
            ));
        }

        let results = parallel::map(edges, params.concurrency, {
            let bar = bar.clone();
            move |(initiator, responder, record)| {
                let bar = bar.clone();
                async move {
                    // Checked per edge rather than once per member: an
                    // earlier round, or an earlier run, may already have
                    // connected this pair.
                    let result = match contact_ids(&initiator).await {
                        Ok(existing) if existing.contains(responder.user_id()) => None,
                        Ok(_) => Some(ops::connect(&initiator, &responder, &record).await),
                        Err(error) => Some(Err(error)),
                    };
                    bar.inc(1);
                    result
                }
            }
        })
        .await;

        for result in results.into_iter().flatten() {
            report.record("mesh_connect", &result);
        }
    }
    bar.finish_with_message("contact mesh built");
    Ok(())
}

async fn invite_everyone(
    fleet: &Fleet,
    chat_id: ChatId,
    invite_batch_size: usize,
    report: &mut Report,
    multi: &MultiProgress,
) -> anyhow::Result<()> {
    let hub = &fleet.members[0].user;
    let hub_participants = hub
        .chat_participants(chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("hub lost its own group right after creating it"))?;

    // Only fleet members, so a stale contact from an earlier, larger run does
    // not get pulled in.
    let fleet_ids: HashSet<UserId> = fleet.members[1..]
        .iter()
        .map(|member| member.user.user_id().clone())
        .collect();
    let hub_contacts = contact_ids(hub).await?;
    let to_invite: Vec<UserId> = fleet_ids
        .into_iter()
        .filter(|id| hub_contacts.contains(id) && !hub_participants.contains(id))
        .collect();

    let batch_size = invite_batch_size.max(1);
    let bar = multi.add(ProgressBar::new(to_invite.len().div_ceil(batch_size) as u64));
    bar.set_style(bar_style("inviting into group"));

    for batch in to_invite.chunks(batch_size) {
        let result = ops::invite(hub, chat_id, batch).await;
        report.record("bootstrap_invite", &result);
        if let Err(error) = result {
            tracing::warn!(%error, batch_size = batch.len(), "bootstrap invite batch failed");
        } else {
            info!(batch_size = batch.len(), "invited batch into group");
        }
        bar.inc(1);
    }
    bar.finish_with_message("invites committed");
    Ok(())
}

/// Has every member consume what bootstrap produced and then commit a key
/// update of its own, followed by a final consumption pass. The walk therefore
/// starts from a group where everyone is at the same epoch and no leaf still
/// holds the key material it joined with.
async fn full_update_sweep(
    fleet: &Fleet,
    chat_id: ChatId,
    concurrency: usize,
    report: &mut Report,
    multi: &MultiProgress,
) {
    let bar = multi.add(ProgressBar::new((fleet.members.len() * 3) as u64));
    bar.set_style(bar_style("full update sweep"));

    // Catch everyone up on the invite batches first. This part is per-member
    // work with nothing shared, so it fans out; it also leaves the drains in
    // the sequential pass below with only one commit each to apply.
    drain_everyone(fleet, concurrency, &bar, report).await;

    // The commits themselves stay sequential: each member drains to the epoch
    // its predecessor's commit created, so every update is staged from the
    // current epoch and a rejection here is a real failure.
    for member in &fleet.members {
        let drain_result = ops::drain(&member.user).await;
        report.record("bootstrap_drain", &drain_result);
        if drain_result.is_ok() && ops::is_member(&member.user, chat_id).await {
            let result = ops::update_key(&member.user, chat_id).await;
            report.record("bootstrap_self_update", &result);
        }
        bar.inc(1);
    }

    // Everyone picks up the commits made after their own turn.
    drain_everyone(fleet, concurrency, &bar, report).await;

    bar.finish_with_message("full update sweep done");
}

/// Drains every member concurrently. Safe to fan out because each task owns a
/// distinct member, so no queue ratchet is touched twice.
async fn drain_everyone(fleet: &Fleet, concurrency: usize, bar: &ProgressBar, report: &mut Report) {
    let results = parallel::map(
        fleet.members.iter().map(|m| m.user.clone()).collect(),
        concurrency,
        {
            let bar = bar.clone();
            move |user| {
                let bar = bar.clone();
                async move {
                    let result = ops::drain(&user).await;
                    bar.inc(1);
                    result
                }
            }
        },
    )
    .await;
    for result in &results {
        report.record("bootstrap_drain", result);
        if let Ok(outcome) = result {
            report.record_drain_outcome(outcome.fetched, outcome.message_errors);
        }
    }
}

async fn contact_ids(user: &CoreUser) -> anyhow::Result<HashSet<UserId>> {
    Ok(user
        .contacts()
        .await?
        .into_iter()
        .map(|contact| contact.user_id)
        .collect())
}

async fn find_group_chat(user: &CoreUser, title: &str) -> anyhow::Result<Option<ChatId>> {
    for chat_id in user.ordered_chat_ids().await? {
        let Some(chat) = user.chat(&chat_id).await else {
            continue;
        };
        if chat.chat_type().is_group()
            && chat.attributes().map(|attrs| attrs.title()) == Some(title)
        {
            return Ok(Some(chat_id));
        }
    }
    Ok(None)
}
