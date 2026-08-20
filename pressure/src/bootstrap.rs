// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Prepares the topology the walk operates on: every member connects to a
//! handful of peers, and `fleet.members[0]` (the hub) creates the group.
//!
//! The group starts with the hub alone. Growing it is the walk's job, which is
//! also what exercises joining: prefilling it here would put the group at the
//! fleet ceiling before the first step, leaving the invite step with nothing to
//! do and the growth bias nothing to bite on.
//!
//! Contacts are a uniform ring, hub included. Adds are contact-gated, so the
//! mesh is what decides who can invite whom, and a ring keeps that symmetric:
//! every member can invite its neighbours, and the group grows outward from the
//! hub as members join and invite their own. The hub is special only in owning
//! the group and being the roster oracle, not in its contacts.
//!
//! Every step checks current local state first, so re-running against an
//! already-bootstrapped fleet only does the work that is still missing.

use std::{collections::HashSet, future::Future};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, UsernameRecord, clients::CoreUser};
use indicatif::{MultiProgress, ProgressBar};
use tracing::info;

use crate::{fleet::Fleet, ops, parallel, progress::bar_style, report::Report};

pub struct BootstrapParams<'a> {
    pub chat_title: &'a str,
    /// Peers each member connects to. Zero leaves the fleet unconnected, so
    /// nobody can invite anybody and the group stays at one member.
    pub contact_mesh_degree: usize,
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

    // Usernames first, for the whole fleet: the mesh needs a record for any
    // member it might connect to. Already-registered members just load their
    // existing record. One member per task, since each only touches its own
    // store.
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

    build_contact_mesh(fleet, &records, params, report, multi).await?;

    let chat_id = match find_group_chat(hub, params.chat_title).await? {
        Some(chat_id) => chat_id,
        None => {
            let chat_id = ops::create_group(hub, params.chat_title.to_owned()).await?;
            info!(%chat_id, "created group");
            chat_id
        }
    };

    // catch_everyone_up(fleet, params.concurrency, report, multi).await;

    Ok(chat_id)
}

/// Connects each member to the `degree` members that follow it in index order,
/// wrapping around. A deterministic ring keeps the mesh stable across resumed
/// runs, and gives every member roughly `2 * degree` contacts once incoming
/// edges are counted.
///
/// Built one offset at a time. Within a single offset the edges `i -> i+offset`
/// form a permutation of the fleet, so every member is the initiator of exactly
/// one edge and the responder of exactly one other. That is what makes the round
/// safe to fan out: no member is ever drained by two tasks at once.
async fn build_contact_mesh(
    fleet: &Fleet,
    records: &[Option<UsernameRecord>],
    params: &BootstrapParams<'_>,
    report: &mut Report,
    multi: &MultiProgress,
) -> anyhow::Result<()> {
    let count = fleet.members.len();
    // Each member can reach at most every other member.
    let degree = params.contact_mesh_degree.min(count.saturating_sub(1));
    if degree == 0 {
        return Ok(());
    }

    let bar = multi.add(ProgressBar::new((degree * count) as u64));
    bar.set_style(bar_style("building contact mesh"));

    for offset in 1..=degree {
        let mut edges = Vec::with_capacity(count);
        for index in 0..count {
            let peer = (index + offset) % count;
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
                    // Checked per edge rather than once per member: an earlier
                    // round, or an earlier run, may already have connected this
                    // pair.
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

    // The mesh decides who can invite whom, so a shortfall here caps how far
    // the walk can ever grow the group -- silently, since the walk just finds no
    // invite candidates. Counted rather than asserted: a partial mesh still
    // makes for a usable run, as long as the run says so.
    // Each edge leaves a contact on both sides, so a complete mesh gives every
    // member `2 * degree` of them.
    let expected = 2 * degree * count;
    let established = fleet
        .members
        .iter()
        .map(|member| async {
            contact_ids(&member.user)
                .await
                .map(|ids| ids.len())
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let total: usize = futures_total(established).await;
    info!(
        expected_edges = expected,
        contact_entries = total,
        members = count,
        "contact mesh established"
    );
    if total < expected {
        tracing::warn!(
            expected_edges = expected,
            contact_entries = total,
            "contact mesh is incomplete; members have fewer contacts than the \
             ring prescribes, which limits how far the walk can grow the group"
        );
    }

    bar.finish_with_message("contact mesh built");
    Ok(())
}

/// Sums a set of futures sequentially. Only used for the post-mesh count, where
/// the reads are cheap and local.
async fn futures_total(futures: Vec<impl Future<Output = usize>>) -> usize {
    let mut total = 0;
    for future in futures {
        total += future.await;
    }
    total
}

/// Has every member consume what the mesh produced, so the walk starts from
/// members that have settled their connection groups.
///
/// Safe to fan out because each task owns a distinct member, so no queue
/// ratchet is touched twice.
async fn catch_everyone_up(
    fleet: &Fleet,
    concurrency: usize,
    report: &mut Report,
    multi: &MultiProgress,
) {
    let bar = multi.add(ProgressBar::new(fleet.members.len() as u64));
    bar.set_style(bar_style("catching members up"));

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
    bar.finish_with_message("members caught up");
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
