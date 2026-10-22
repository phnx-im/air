// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod client_records;
pub(crate) mod key_packages;
pub(crate) mod user_records;

#[cfg(test)]
pub(crate) mod tests {
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{air_service::BackendService, qs::Qs};

    pub(crate) async fn qs(pool: &PgPool) -> anyhow::Result<Qs> {
        let qs = Qs::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;
        Ok(qs)
    }
}
