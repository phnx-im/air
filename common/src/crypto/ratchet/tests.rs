// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    messages::client_ds::{QsQueueMessagePayload, QsQueueMessageType},
    time::TimeStamp,
};

use super::*;

// Test that the ratchet works.
#[test]
fn test_ratchet() {
    let ratchet_secret = RatchetSecret::random().unwrap();
    let mut sender_ratchet = QueueRatchet::try_from(ratchet_secret.clone()).unwrap();
    let mut receiver_ratchtet = QueueRatchet::try_from(ratchet_secret).unwrap();
    let message = QsQueueMessagePayload {
        timestamp: TimeStamp::now(),
        message_type: QsQueueMessageType::MlsMessage,
        payload: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    };
    let encrypted_message = sender_ratchet.encrypt(&message).unwrap();
    let plaintext: QsQueueMessagePayload = receiver_ratchtet.decrypt(encrypted_message).unwrap();
    assert_eq!(plaintext, message);
}
