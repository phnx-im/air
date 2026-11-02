// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_as::ConnectionOfferHash;
use anyhow::Result;
use openmls::{
    prelude::OpenMlsProvider,
    schedule::{ExternalPsk, PreSharedKeyId, Psk},
};
use openmls_traits::storage::StorageProvider;

use crate::{
    db::access::WriteConnection,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    pub(crate) fn store_connection_offer_psk(
        &self,
        mut connection: impl WriteConnection,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        store_connection_offer_psk(
            &provider,
            self.mls_group().ciphersuite(),
            connection_offer_hash,
        )?;
        Ok(())
    }

    pub(crate) fn delete_connection_offer_psk(
        mut connection: impl WriteConnection,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let psk = Psk::External(ExternalPsk::new(
            connection_offer_hash.into_bytes().to_vec(),
        ));
        provider.storage().delete_psk(&psk)?;
        Ok(())
    }
}

/// Stores the connection-offer PSK, so a commit referencing it can be created
/// or processed. Returns the id the commit's PSK proposal must carry.
pub(super) fn store_connection_offer_psk(
    provider: &AirOpenMlsProvider<'_>,
    ciphersuite: openmls::prelude::Ciphersuite,
    connection_offer_hash: ConnectionOfferHash,
) -> Result<PreSharedKeyId> {
    let psk_value = connection_offer_hash.into_bytes();
    let psk_id = PreSharedKeyId::new(
        ciphersuite,
        provider.rand(),
        Psk::External(ExternalPsk::new(psk_value.to_vec())),
    )?;
    psk_id.store(provider, &psk_value)?;
    Ok(psk_id)
}
