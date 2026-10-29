// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::LeafSigningKey,
    identifiers::QsReference,
    messages::{
        client_ds::AadMessage,
        client_ds_out::{ApqGroupOperationParamsOut, GroupOperationParamsOut},
    },
    mls_group_config::{
        GROUP_DATA_EXTENSION_TYPE, QS_CLIENT_REFERENCE_EXTENSION_TYPE,
        default_leaf_node_capabilities, self_group_leaf_node_capabilities,
    },
};
use airprotos::client::{
    app_data::{ClientAppData, GroupAppDataExt},
    component::AIR_GROUP_PROFILE_COMPONENT_ID,
    group::GroupData,
};
use anyhow::{Context, Result, anyhow, ensure};
use mls_assist::messages::AssistedMessageOut;
use openmls::{
    component::ComponentData,
    group::GroupContext,
    prelude::{
        AppDataDictionaryExtension, AppDataUpdateProposal, Capabilities, Extension, Extensions,
        LeafNode, LeafNodeParameters, OpenMlsProvider, Proposal, UnknownExtension,
        tls_codec::Serialize as TlsSerializeTrait,
    },
};

use crate::{
    chats::GroupDataExt,
    db::access::WriteDbTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
    job::chat_operation::DerivationEpoch,
};

impl Group {
    pub(crate) async fn update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
        new_group_data: Option<GroupData>,
        derivation_epoch: DerivationEpoch,
        own_client_reference: &QsReference,
    ) -> Result<GroupOperationParamsOut> {
        // We don't expect there to be a welcome.
        let aad = AadMessage::empty_group_operation().tls_serialize_detached()?;

        let (component_data, extensions) = self.group_data_update(new_group_data)?;

        let own_leaf_node = self.mls_group.own_leaf_node().context("No own leaf node")?;
        let leaf_node_parameters = Self::update_leaf_node_extensions(
            own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
            self.is_self_group().then_some(own_client_reference),
        )?;

        // A leaf shared with sibling emulator clients must be replaced with key
        // material derived from the emulation epoch, or the siblings cannot
        // rederive it and drop out of the group.
        let vc_group_id = self.resolve_vc_emulation_group(&mut *txn).await?;

        self.mls_group.set_aad(aad);
        let (mls_message, group_info) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());

            let mut builder = self.mls_group.commit_builder();
            if let Some(extensions) = extensions {
                builder = builder.propose_group_context_extensions(extensions)?;
            };

            let mut builder = builder
                .force_self_update(true)
                .leaf_node_parameters(leaf_node_parameters)
                .derivation_epoch(derivation_epoch.rotates());
            if let Some(group_id) = &vc_group_id {
                builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
            }

            if let Some(component_data) = &component_data {
                builder = builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                    AppDataUpdateProposal::update(
                        AIR_GROUP_PROFILE_COMPONENT_ID,
                        component_data.data().to_vec(),
                    ),
                )));
            }

            let mut builder = builder.load_psks(provider.storage())?;

            if let Some(component_data) = component_data {
                let mut updater = builder.app_data_dictionary_updater();
                updater.set(component_data);
                builder.with_app_data_dictionary_updates(updater.changes());
            }

            let (mls_message, _welcome_option, group_info_option) = builder
                .create_group_info(true)
                .build(provider.rand(), provider.crypto(), signer, |_| true)?
                .stage_commit(&provider)?
                .into_contents();

            (
                mls_message,
                group_info_option.ok_or_else(|| anyhow!("No group info after commit"))?,
            )
        };

        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));
        Ok(GroupOperationParamsOut {
            commit,
            add_users_info_option: None,
        })
    }

    /// If the group has a group profile component, `new_group_data` goes there.
    /// Otherwise it goes into the group context extension.
    fn group_data_update(
        &self,
        new_group_data: Option<GroupData>,
    ) -> Result<(Option<ComponentData>, Option<Extensions<GroupContext>>)> {
        let Some(group_data) = new_group_data else {
            return Ok((None, None));
        };
        if self.mls_group.extensions().has_group_profile_component() {
            let component_data = group_data.into_component().to_component_data()?;
            Ok((Some(component_data), None))
        } else {
            let bytes = group_data.encode()?;
            let mut extensions = self.mls_group().extensions().clone();
            extensions.add_or_replace(Extension::Unknown(
                GROUP_DATA_EXTENSION_TYPE,
                UnknownExtension(bytes),
            ))?;
            Ok((None, Some(extensions)))
        }
    }

    /// APQ self-update on both the T and PQ groups.
    ///
    /// Produces a single combined commit via apqmls that forces a self-update of the key material
    /// in both groups. Return [`ApqGroupOperationParamsOut`] so the caller can persist it as
    /// `ApqOther` pending chat operation.
    pub(crate) async fn apq_update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
        new_group_data: Option<GroupData>,
        derivation_epoch: DerivationEpoch,
        own_client_reference: &QsReference,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        let aad = AadMessage::empty_group_operation().tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let (component_data, group_context_extensions) = self.group_data_update(new_group_data)?;

        let own_client_reference = self.is_self_group().then_some(own_client_reference);
        let t_own_leaf_node = self.mls_group.own_leaf_node().context("No own leaf node")?;
        let t_leaf_node_parameters = Self::update_leaf_node_extensions(
            t_own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
            own_client_reference,
        )?;
        let pq_own_leaf_node = self
            .pq()
            .context("No PQ group found")?
            .mls_group
            .own_leaf_node()
            .context("No own PQ leaf node")?;
        let pq_leaf_node_parameters = Self::update_leaf_node_extensions(
            pq_own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
            own_client_reference,
        )?;

        let vc_group_id = self.resolve_vc_emulation_group(&mut *txn).await?;

        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;
        let mut builder =
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .leaf_node_parameters(t_leaf_node_parameters, pq_leaf_node_parameters)
                .derivation_epoch(derivation_epoch.rotates())
                .create_group_info(true);
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        if let Some(extensions) = group_context_extensions {
            builder = builder.propose_t_group_context_extensions(extensions);
        }
        if let Some(component_data) = component_data {
            builder = builder.update_t_app_data(component_data);
        }
        let bundle = builder.finalize(&provider, signer, |_| true, |_| true)?;

        debug_assert!(bundle.welcome.is_none());
        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        Ok(ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos: Vec::new(),
        })
    }

    /// Capabilities for the own leaf when a commit sets explicit leaf node parameters.
    ///
    /// Self-group leaves must keep advertising the self-group credential type, see
    /// [`self_group_leaf_node_capabilities`].
    fn own_leaf_capabilities(&self) -> Capabilities {
        if self.is_self_group() {
            self_group_leaf_node_capabilities()
        } else {
            default_leaf_node_capabilities()
        }
    }

    fn update_leaf_node_extensions(
        leaf_node_extensions: &Extensions<LeafNode>,
        capabilities: Capabilities,
        client_reference: Option<&QsReference>,
    ) -> anyhow::Result<LeafNodeParameters> {
        let mut extensions = leaf_node_extensions.clone();

        // Refresh the app data to what this version of the client advertises. Only touch the
        // leaf node extensions if something actually changed.
        let refreshed = leaf_node_extensions.app_data_dictionary().map(|current| {
            let mut dict = current.dictionary().clone();
            ClientAppData::refresh(&mut dict);
            (&dict != current.dictionary()).then_some(dict)
        });
        match refreshed {
            // Up to date
            Some(None) => {}
            Some(Some(dict)) => {
                extensions.add_or_replace(Extension::AppDataDictionary(
                    AppDataDictionaryExtension::new(dict),
                ))?;
            }
            None => {
                // App data extension is not present, add it with default values
                extensions.add(ClientAppData::current().to_extension())?;
            }
        }

        // Self groups created by older clients have no client reference on the
        // creator leaf. Also, clients before 0.23 could drop it during an APQ
        // self-update. We add it back so that every leaf of the self group
        // contains one.
        if let Some(client_reference) = client_reference
            && extensions
                .unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE)
                .is_none()
        {
            extensions.add(client_reference_extension(client_reference)?)?;
        }

        let mut leaf_node_parameters =
            LeafNodeParameters::builder().with_capabilities(capabilities);
        if &extensions != leaf_node_extensions {
            leaf_node_parameters = leaf_node_parameters.with_extensions(extensions);
        }
        Ok(leaf_node_parameters.build())
    }
}

pub(crate) fn client_reference_extension(
    client_reference: &QsReference,
) -> Result<Extension, tls_codec::Error> {
    // TODO: don't use Extension::Unknown
    Ok(Extension::Unknown(
        QS_CLIENT_REFERENCE_EXTENSION_TYPE,
        UnknownExtension(client_reference.tls_serialize_detached()?),
    ))
}

#[cfg(test)]
mod tests {
    use aircommon::mls_group_config::{
        QS_CLIENT_REFERENCE_EXTENSION_TYPE, default_leaf_node_capabilities,
        self_group_leaf_node_capabilities,
    };
    use airprotos::client::{app_data::ClientAppData, component::AirFeatures};
    use openmls::prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions, LeafNode,
    };

    use super::client_reference_extension;
    use crate::{groups::Group, utils::random_client_reference};

    fn extensions_with_dict(dict: AppDataDictionary) -> Extensions<LeafNode> {
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("valid extensions")
    }

    /// No app data dictionary -> add the current one
    #[test]
    fn no_app_data_dictionary() {
        let extensions = Extensions::empty();
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities(), None)
                .unwrap();
        assert_eq!(
            params.extensions(),
            Some(&ClientAppData::current().leaf_node_extensions())
        );
    }

    /// Empty app data dictionary -> filled with the current one
    #[test]
    fn empty_app_data_dictionary() {
        let extensions = extensions_with_dict(AppDataDictionary::new());
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities(), None)
                .unwrap();
        assert_eq!(
            params.extensions(),
            Some(&ClientAppData::current().leaf_node_extensions())
        );
    }

    #[test]
    fn stale_app_data_is_refreshed() {
        let stale = ClientAppData {
            features: AirFeatures::default(),
            virtual_client: false,
        };
        let extensions = stale.leaf_node_extensions();
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities(), None)
                .unwrap();
        assert_eq!(
            params.extensions(),
            Some(&ClientAppData::current().leaf_node_extensions())
        );
    }

    /// Up-to-date app data -> extensions in params are unchanged (None)
    #[test]
    fn up_to_date_app_data_is_left_alone() {
        let extensions = ClientAppData::current().leaf_node_extensions();
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities(), None)
                .unwrap();
        assert!(params.extensions().is_none());
    }

    #[test]
    fn missing_client_reference_is_added() {
        let extensions = ClientAppData::current().leaf_node_extensions();
        let client_reference = random_client_reference();
        let params = Group::update_leaf_node_extensions(
            &extensions,
            self_group_leaf_node_capabilities(),
            Some(&client_reference),
        )
        .unwrap();

        let mut expected = extensions.clone();
        expected
            .add(client_reference_extension(&client_reference).unwrap())
            .unwrap();
        assert_eq!(params.extensions(), Some(&expected));
    }

    #[test]
    fn existing_client_reference_is_kept() {
        let mut extensions = ClientAppData::current().leaf_node_extensions();
        extensions
            .add(client_reference_extension(&random_client_reference()).unwrap())
            .unwrap();
        let params = Group::update_leaf_node_extensions(
            &extensions,
            self_group_leaf_node_capabilities(),
            Some(&random_client_reference()),
        )
        .unwrap();
        assert!(params.extensions().is_none());
    }

    #[test]
    fn client_reference_is_added_alongside_refreshed_app_data() {
        let extensions = Extensions::empty();
        let client_reference = random_client_reference();
        let params = Group::update_leaf_node_extensions(
            &extensions,
            self_group_leaf_node_capabilities(),
            Some(&client_reference),
        )
        .unwrap();

        let extensions = params.extensions().expect("extensions changed");
        assert!(extensions.app_data_dictionary().is_some());
        assert!(
            extensions
                .unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE)
                .is_some()
        );
    }
}
