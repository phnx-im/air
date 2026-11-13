// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A second AS-issued credential for a user id that is already a member.

use airserver_test_harness::utils::setup::TestBackend;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn second_credential_for_known_user_id_is_rejected() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, carol, victim] = setup.add_users().await;
    setup.connect_users_non_apq(&alice, &bob).await;
    setup.connect_users_non_apq(&alice, &carol).await;
    setup.connect_users_non_apq(&alice, &victim).await;
    let chat_id = setup.create_non_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &carol, &victim])
        .await;
    // Bob's removal leaves the blank leaf the openmls builder needs to place
    // the new leaf where receivers expect it.
    setup
        .remove_from_group(chat_id, &alice, &[&bob])
        .await
        .unwrap();

    let victim_id = victim.user_id().clone();
    let code_before = alice.safety_code(&victim_id).await.unwrap();
    let members_before = alice.mls_members(chat_id).await.unwrap().unwrap();
    let messages_before = alice.messages(chat_id, 1024).await.unwrap().len();

    let (minted, signer) = setup.mint_user_credential(&victim_id).await;
    let (commit, message) = carol
        .forge_credential_resync(chat_id, &victim_id, &minted, &signer, "sent by the server")
        .await
        .unwrap();

    let Err(error) = alice.process_incoming_mls_message(&commit).await else {
        panic!("Alice must reject a second key for a known user id");
    };
    assert!(
        format!("{error:#}").contains("another key than the one pinned"),
        "unexpected error: {error:#}"
    );
    // The message belongs to the epoch of the rejected commit. Whether processing drops it or
    // fails, the message count below shows that it is not stored.
    let _ = alice.process_incoming_mls_message(&message).await;

    let members_after = alice.mls_members(chat_id).await.unwrap().unwrap();
    assert_eq!(members_after, members_before);
    assert_eq!(alice.user_credential_count(&victim_id).await.unwrap(), 1);
    assert_eq!(alice.safety_code(&victim_id).await.unwrap(), code_before);
    assert_eq!(
        alice.messages(chat_id, 1024).await.unwrap().len(),
        messages_before
    );
}
