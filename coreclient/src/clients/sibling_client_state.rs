// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Notifications suppressed by sibling clients.
//!
//! Every listening client relays which notifications it suppresses, because
//! the user is looking at it, to its siblings through the QS. A client does not
//! notify about what a sibling suppresses.
//!
//! The states are only valid for the listen stream they were received on, so
//! they are kept by the owner of the stream.

use std::collections::HashMap;

use aircommon::{
    crypto::{aead::keys::ClientStateKey, kdf::KdfDerivable},
    identifiers::QsClientId,
    messages::client_state::{ClientState, NotificationSuppression},
};
use airprotos::queue_service::v1::{self, sibling_client_state};
use anyhow::Context;
use tracing::{debug, warn};

use crate::{ChatId, clients::CoreUser};

#[derive(Debug)]
pub struct SiblingClientStates {
    key: ClientStateKey,
    /// Last epoch and suppression per sibling
    states: HashMap<QsClientId, (u64, NotificationSuppression)>,
}

impl SiblingClientStates {
    fn new(key: ClientStateKey) -> Self {
        Self {
            key,
            states: HashMap::new(),
        }
    }

    /// Records a change of a sibling client state received from the QS.
    pub fn apply(&mut self, state: v1::SiblingClientState) {
        let (client_id, epoch, suppression) = match state.change {
            Some(sibling_client_state::Change::Updated(updated)) => {
                let Some(client_id) = updated.client_id.and_then(|id| id.try_into().ok()) else {
                    warn!("sibling client state without client id");
                    return;
                };
                let blob = updated.blob.unwrap_or_default().encrypted_blob;
                let suppression =
                    match ClientState::decrypt_from_bytes(&self.key, &client_id, &blob) {
                        Ok(state) => state.suppression(),
                        Err(error) => {
                            warn!(%error, "failed to decrypt sibling client state");
                            NotificationSuppression::None
                        }
                    };
                debug!(?suppression, "applying sibling client update");
                (client_id, updated.epoch, suppression)
            }
            Some(sibling_client_state::Change::Removed(removed)) => {
                let Some(client_id) = removed.client_id.and_then(|id| id.try_into().ok()) else {
                    warn!("sibling client state without client id");
                    return;
                };
                debug!(?client_id, "removing sibling client state");
                (client_id, removed.epoch, NotificationSuppression::None)
            }
            None => return,
        };

        // The initial states of a session can race with live changes.
        if self
            .states
            .get(&client_id)
            .is_some_and(|(last_epoch, _)| *last_epoch >= epoch)
        {
            return;
        }
        self.states.insert(client_id, (epoch, suppression));
    }

    /// Whether a sibling suppresses notifications of `chat_id`.
    pub fn suppresses(&self, chat_id: ChatId) -> bool {
        self.states
            .values()
            .any(|(_, suppression)| match suppression {
                NotificationSuppression::None | NotificationSuppression::Unknown => false,
                NotificationSuppression::Chat(id) => *id == chat_id.uuid(),
                NotificationSuppression::All => true,
            })
    }
}

impl CoreUser {
    fn client_state_key(&self) -> anyhow::Result<ClientStateKey> {
        let secret = self.key_store().qs_user_signing_key.derive_sibling_secret();
        ClientStateKey::derive(&secret, &Vec::new()).context("failed to derive client state key")
    }

    /// Encrypts the state of this client with the notifications it suppresses,
    /// for relaying it to the siblings.
    pub fn encrypt_client_state(
        &self,
        suppression: NotificationSuppression,
    ) -> anyhow::Result<Vec<u8>> {
        ClientState::new(suppression)
            .encrypt_to_bytes(&self.client_state_key()?, &self.inner.qs_client_id)
            .context("failed to encrypt client state")
    }

    /// Starts tracking the sibling client states of a new listen stream.
    pub fn sibling_client_states(&self) -> anyhow::Result<SiblingClientStates> {
        Ok(SiblingClientStates::new(self.client_state_key()?))
    }
}

#[cfg(test)]
mod tests {
    use aircommon::crypto::signatures::keys::QsUserSigningKey;
    use airprotos::queue_service::v1::{
        SiblingClientStateEncryptedBlob, SiblingClientStateRemoved, SiblingClientStateUpdated,
    };
    use uuid::Uuid;

    use super::*;

    fn key() -> ClientStateKey {
        let signing_key = QsUserSigningKey::generate().unwrap();
        ClientStateKey::derive(&signing_key.derive_sibling_secret(), &Vec::new()).unwrap()
    }

    fn updated(
        key: &ClientStateKey,
        client_id: QsClientId,
        epoch: u64,
        suppression: NotificationSuppression,
    ) -> v1::SiblingClientState {
        let encrypted_blob = ClientState::new(suppression)
            .encrypt_to_bytes(key, &client_id)
            .unwrap();
        v1::SiblingClientState {
            change: Some(sibling_client_state::Change::Updated(
                SiblingClientStateUpdated {
                    client_id: Some(client_id.into()),
                    epoch,
                    blob: Some(SiblingClientStateEncryptedBlob { encrypted_blob }),
                },
            )),
        }
    }

    fn removed(client_id: QsClientId, epoch: u64) -> v1::SiblingClientState {
        v1::SiblingClientState {
            change: Some(sibling_client_state::Change::Removed(
                SiblingClientStateRemoved {
                    client_id: Some(client_id.into()),
                    epoch,
                },
            )),
        }
    }

    #[test]
    fn union_of_sibling_suppressions() {
        let key = key();
        let a = QsClientId::random(&mut rand::rng());
        let b = QsClientId::random(&mut rand::rng());
        let chat = ChatId::new(Uuid::new_v4());
        let other_chat = ChatId::new(Uuid::new_v4());
        let mut states = SiblingClientStates::new(key.clone());

        states.apply(updated(
            &key,
            a,
            1,
            NotificationSuppression::Chat(chat.uuid()),
        ));
        states.apply(updated(
            &key,
            b,
            1,
            NotificationSuppression::Chat(chat.uuid()),
        ));
        assert!(states.suppresses(chat));
        assert!(!states.suppresses(other_chat));

        // One sibling moving away keeps the chat suppressed.
        states.apply(updated(&key, a, 2, NotificationSuppression::None));
        assert!(states.suppresses(chat));

        states.apply(updated(&key, a, 3, NotificationSuppression::All));
        assert!(states.suppresses(other_chat));

        states.apply(removed(a, 4));
        states.apply(removed(b, 2));
        assert!(!states.suppresses(chat));
    }

    #[test]
    fn stale_changes_are_ignored() {
        let key = key();
        let a = QsClientId::random(&mut rand::rng());
        let chat = ChatId::new(Uuid::new_v4());
        let mut states = SiblingClientStates::new(key.clone());

        states.apply(updated(
            &key,
            a,
            2,
            NotificationSuppression::Chat(chat.uuid()),
        ));
        states.apply(updated(&key, a, 1, NotificationSuppression::None));
        assert!(states.suppresses(chat));

        states.apply(removed(a, 3));
        states.apply(updated(&key, a, 2, NotificationSuppression::All));
        assert!(!states.suppresses(chat));
    }
}
