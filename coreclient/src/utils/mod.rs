// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod file_lock;
pub(crate) mod global_lock;
pub(crate) mod image;
pub(crate) mod persistence;

#[cfg(test)]
pub(crate) fn init_test_tracing() {
    use tracing::Level;
    use tracing_subscriber::EnvFilter;

    let _ = tracing_subscriber::fmt::fmt()
        .with_test_writer()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .try_init();
}

/// A reference to a random QS client, sealed with a random key.
#[cfg(test)]
pub(crate) fn random_client_reference() -> aircommon::identifiers::QsReference {
    use aircommon::{
        crypto::hpke::{ClientIdDecryptionKey, HpkeEncryptable},
        identifiers::{ClientConfig, QsClientId, QsReference},
    };

    QsReference {
        client_homeserver_domain: "example.com".parse().unwrap(),
        sealed_reference: ClientConfig {
            client_id: QsClientId::random(&mut rand::rng()),
            push_token_ear_key: None,
        }
        .encrypt(
            ClientIdDecryptionKey::generate().unwrap().encryption_key(),
            &[],
            &[],
        ),
    }
}
