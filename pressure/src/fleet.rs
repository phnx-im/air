// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Builds and resumes a fleet of persisted [`CoreUser`]s against a shared
//! server. Each member lives in its own subdirectory of the fleet root, which
//! doubles as the resume marker: a directory that already contains a client
//! record is loaded instead of recreated, so a killed run can continue
//! against the same accounts and groups.

use std::path::{Path, PathBuf};

use aircommon::identifiers::{Fqdn, UserId};
use aircoreclient::clients::{CoreUser, store::ClientRecord};
use anyhow::Context;
use indicatif::{MultiProgress, ProgressBar};
use tracing::info;
use url::Url;

use crate::{ops, progress::bar_style};

pub struct FleetMember {
    pub index: usize,
    pub user: CoreUser,
}

/// What one member needs before the fan-out starts, decided up front so the
/// work inside a task depends on nothing outside itself.
struct MemberPlan {
    index: usize,
    dir: PathBuf,
    resuming: bool,
    code: Option<String>,
}

pub struct Fleet {
    pub members: Vec<FleetMember>,
}

impl Fleet {
    /// Loads or creates `count` members under `root`. `invitation_codes` is
    /// consumed one code per freshly created member; it is fine to pass an
    /// empty list when the server has invitations disabled.
    pub async fn load_or_create(
        root: &Path,
        server_url: Url,
        count: usize,
        mut invitation_codes: Option<Vec<String>>,
        concurrency: usize,
        multi: &MultiProgress,
    ) -> anyhow::Result<Self> {
        let domain: Fqdn = server_url
            .host()
            .context("server URL has no host")?
            .to_owned()
            .into();

        std::fs::create_dir_all(root)
            .with_context(|| format!("failed to create fleet root {}", root.display()))?;

        // Invitation codes are handed out here rather than inside the tasks,
        // so which member gets which code stays deterministic regardless of
        // the order the fan-out happens to run in.
        let mut plans = Vec::with_capacity(count);
        for index in 0..count {
            let dir = root.join(format!("user-{index:05}"));
            let resuming = dir.exists();
            let code = if resuming {
                None
            } else {
                invitation_codes
                    .as_mut()
                    .map(|codes| {
                        codes.pop().with_context(|| {
                            format!(
                                "no invitation code left to create member {index} \
                         (need one per freshly created member; generate more with \
                         `airserver code generate <n>` on the target server)"
                            )
                        })
                    })
                    .transpose()?
            };
            plans.push(MemberPlan {
                index,
                dir,
                resuming,
                code,
            });
        }

        let created = plans.iter().filter(|plan| !plan.resuming).count();
        let resumed = count - created;

        let bar = multi.add(ProgressBar::new(count as u64));
        bar.set_style(bar_style("preparing fleet"));
        bar.set_message(format!("{created} to create, {resumed} to resume"));

        // Members share nothing: their own directory, their own lock file,
        // their own queues. The crypto each one does on creation is what
        // makes this the most expensive phase, so it is spread over the
        // runtime's workers.
        let results = crate::parallel::map(plans, concurrency, {
            let server_url = server_url.clone();
            let domain = domain.clone();
            let bar = bar.clone();
            move |plan| {
                let server_url = server_url.clone();
                let domain = domain.clone();
                let bar = bar.clone();
                async move {
                    let outcome = Self::prepare_member(&plan, domain, server_url).await;
                    bar.inc(1);
                    (plan, outcome)
                }
            }
        })
        .await;

        let mut members = Vec::with_capacity(count);
        for (plan, outcome) in results {
            let user = outcome?;
            info!(index = plan.index, dir = ?plan.dir, user_id = ?user.user_id(), "member ready");
            members.push(FleetMember {
                index: plan.index,
                user,
            });
        }
        bar.finish_with_message(format!("{created} created, {resumed} resumed"));

        Ok(Self { members })
    }

    async fn prepare_member(
        plan: &MemberPlan,
        domain: Fqdn,
        server_url: Url,
    ) -> anyhow::Result<CoreUser> {
        if plan.resuming {
            return Self::resume_member(&plan.dir, server_url)
                .await
                .with_context(|| {
                    format!("failed to resume member {} from {:?}", plan.index, plan.dir)
                });
        }

        std::fs::create_dir_all(&plan.dir)
            .with_context(|| format!("failed to create member dir {:?}", plan.dir))?;
        let user = Self::create_member(&plan.dir, domain, server_url, plan.code.clone())
            .await
            .with_context(|| format!("failed to create member {} in {:?}", plan.index, plan.dir))?;
        // Only on creation, while the member is still in no groups. Setting a
        // profile rotates the user profile key and pushes it to the DS once
        // per group the member belongs to, so doing this on resume would fire
        // a fleet-wide burst of profile key updates just to restate a
        // cosmetic label.
        ops::ensure_profile(&user, plan.index)
            .await
            .with_context(|| format!("failed to set profile for member {}", plan.index))?;
        Ok(user)
    }

    async fn create_member(
        dir: &Path,
        domain: Fqdn,
        server_url: Url,
        invitation_code: Option<String>,
    ) -> anyhow::Result<CoreUser> {
        let user_id = UserId::random(domain);
        let dir_str = dir.to_str().context("fleet dir path is not valid UTF-8")?;
        let user = CoreUser::with_server_url(
            user_id,
            Some(server_url),
            dir_str,
            None,
            invitation_code.unwrap_or_default(),
        )
        .await?;
        // Upload this member's initial key packages so others can add/connect to it.
        user.outbound_service().run_once().await;
        Ok(user)
    }

    async fn resume_member(dir: &Path, server_url: Url) -> anyhow::Result<CoreUser> {
        let dir_str = dir.to_str().context("fleet dir path is not valid UTF-8")?;
        let records = ClientRecord::load_all_from_air_db(dir_str).await?;
        let record = match records.as_slice() {
            [record] => record,
            [] => anyhow::bail!("no client record found in {dir:?}"),
            _ => anyhow::bail!(
                "expected exactly one client record in {dir:?}, found {}",
                records.len()
            ),
        };
        let user =
            CoreUser::load_with_server_url(dir_str, record.client_record_id, Some(server_url))
                .await?;
        user.outbound_service().run_once().await;
        Ok(user)
    }
}

/// Reads invitation codes from a file, one per line, ignoring blank lines.
pub fn read_invitation_codes(path: &Path) -> anyhow::Result<Vec<String>> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read invitation codes from {path:?}"))?;
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}
