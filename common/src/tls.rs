// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! TLS setup shared by the client and the server.

/// Installs aws-lc-rs as the process-wide rustls crypto provider, unless a
/// provider is already installed.
///
/// reqwest takes its TLS configuration from the process-wide provider and
/// panics when building a client without one. aws-lc-rs offers the hybrid
/// post-quantum key exchange X25519MLKEM768 first.
pub fn install_crypto_provider() {
    // An error only means that a provider is already installed.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use rustls::NamedGroup;

    #[test]
    fn default_provider_prefers_post_quantum_key_exchange() {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        let first_group = provider.kx_groups.first().map(|group| group.name());
        assert_eq!(first_group, Some(NamedGroup::X25519MLKEM768));
    }
}
