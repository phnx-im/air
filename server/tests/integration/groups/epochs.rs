// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Past-epoch decryption limit and ratchet tolerance.

use aircommon::mls_group_config::MAX_PAST_EPOCHS;
use aircoreclient::{
    ChatId,
    clients::{CoreUser, MarkChatAsRead, process::process_qs::ProcessedQsMessages},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn max_past_epochs() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // To test proper handling of application messages from past epochs, we have
    // Alice locally create updates without sending them to the server. Bob can then
    // send messages based on his (old) epoch for Alice to process.

    // Create MAX_PAST_EPOCHS updates and send a message from Bob to Alice after
    // each update.
    for _ in 0..MAX_PAST_EPOCHS {
        let result = update_and_send_message(contact_chat_id, &alice, &bob).await;
        assert!(
            result.errors.is_empty(),
            "Alice should process Bob's message without errors"
        );
    }

    // Repeat one more time, this time we expect an error
    let result = update_and_send_message(contact_chat_id, &alice, &bob).await;
    let error = &result.errors[0].to_string();
    assert_eq!(
        error.to_string(),
        "Could not process message: ValidationError(UnableToDecrypt(SecretTreeError(TooDistantInThePast)))".to_string(),
        "Alice should fail to process Bob's message with a TooDistantInThePast error"
    );
}

async fn update_and_send_message(
    contact_chat_id: ChatId,
    alice: &CoreUser,
    bob: &CoreUser,
) -> ProcessedQsMessages {
    // alice creates an update and sends it to the ds
    alice.update_key(contact_chat_id).await.unwrap();
    // bob creates a message based on his (old) epoch for alice
    bob.send_text(contact_chat_id, "message").await;
    // alice fetches and processes bob's message
    let qs_messages = alice.qs_fetch_messages().await.unwrap();
    alice.fully_process_qs_messages(qs_messages).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn ratchet_tolerance() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // To test the tolerance of the ratchet, we have Alice send a bunch of
    // messages and then give Bob only the last one to process.
    for _ in 0..5 {
        let msg = MimiContent::simple_markdown_message("message".to_owned(), [0; 16]);
        alice
            .send_message(contact_chat_id, msg, None, MarkChatAsRead::Yes)
            .await
            .unwrap();
    }
    alice.outbound_service().run_once().await;

    let mut qs_messages = bob.qs_fetch_messages().await.unwrap();
    // Give Bob only the last message to process
    let last_message = qs_messages.pop().unwrap();
    let result = bob.fully_process_qs_messages(vec![last_message]).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's last message without errors"
    );
}
