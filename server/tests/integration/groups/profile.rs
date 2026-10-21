// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group data and the group profile component: updates and fetching on invite
//! and update.

use std::slice;

use aircoreclient::{EventMessage, Message, SystemMessage};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use crate::messaging::attachments::test_picture_bytes;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_picture_and_title_updates_reach_members() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let _alice_charlie_chat = setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    // Alice updates the group picture
    let picture = test_picture_bytes();
    alice
        .set_chat_picture(chat_id, Some(picture.clone()))
        .await
        .unwrap();

    let expected_picture = alice
        .expect_chat(chat_id)
        .await
        .attributes()
        .unwrap()
        .picture()
        .unwrap()
        .to_owned();

    // Bob and Charlie should now have the updated group picture
    for user in [&bob, &charlie] {
        // Fetch and process messages to get the update
        user.drain().await;
        user.outbound_service().run_once().await;
        let actual_picture = user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .picture()
            .unwrap()
            .to_owned();
        assert_eq!(actual_picture, expected_picture);
    }

    // Now Bob updates the group title
    let title = "New Group Title".to_string();
    bob.set_chat_title(chat_id, title.clone()).await.unwrap();

    for user in [&alice, &charlie] {
        // Fetch and process messages to get the update
        user.drain().await;
        let actual_title = user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title
            .clone();
        assert_eq!(actual_title, title);
    }
}

/// Regression test for the `OwnPendingCommit` merge path.
///
/// When we commit (here: a title change), the DS both sends a `DsCommitResponse`
/// and echoes the commit back to us via fanout. If the echo is processed before
/// the inline merge (a real race, guarded by `is_pending_for_chat`), it hits the
/// `OwnPendingCommit` arm instead of `DsCommitResponse`. That arm must apply the
/// same side effects: update the chat title carried by the commit and delete the
/// pending chat operation. This test drives that arm directly by staging the
/// commit without merging and feeding the echo back through the QS path.
///
/// This is what would happen in the scenario of virtual-clients.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn own_commit_echo_applies_group_data() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    // Stage Alice's title-change commit WITHOUT merging it. This leaves a
    // pending commit and a stored pending chat operation, and hands back the
    // commit bytes the DS would echo back to Alice.
    let title = "Raced Title".to_string();
    let echo = alice
        .stage_group_title_commit(chat_id, title.clone())
        .await
        .unwrap();

    // Preconditions: pending op exists, title not yet applied.
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_some(),
        "a pending chat operation should be stored before the merge",
    );
    assert_ne!(
        alice.expect_chat(chat_id).await.attributes().unwrap().title,
        title,
        "title must not be applied before the commit is merged",
    );

    // Feed Alice her own commit back through the QS path while the commit is
    // still pending -> hits the `OwnPendingCommit` arm.
    alice.process_incoming_mls_message(&echo).await.unwrap();

    // The arm must apply the group data (title) ...
    assert_eq!(
        alice.expect_chat(chat_id).await.attributes().unwrap().title,
        title,
        "OwnPendingCommit must apply the group data carried by the commit",
    );
    // ... and delete the pending chat operation.
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "OwnPendingCommit must delete the pending chat operation",
    );
}

/// Tests that after being invited to a group, the invitee fetches the encrypted group profile from
/// object storage via the outbound service and sees the correct group attributes (title and
/// picture).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invitee_fetches_group_profile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup.create_group(&alice).await;

    alice
        .set_chat_picture(chat_id, Some(test_picture_bytes()))
        .await
        .unwrap();

    // Record Alice's group attributes (title and resized picture)
    let alice_chat = alice.expect_chat(chat_id).await;
    let attributes = alice_chat.attributes().unwrap().clone();
    let expected_title = attributes.title;
    let expected_picture = attributes.picture;

    // Alice invites Bob; the encrypted group profile is already uploaded from group creation
    alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .unwrap();

    // Bob processes the invitation: this schedules a FetchGroupProfileOperation
    bob.drain().await;

    // Bob sees the group title immediately after processing the invitation, but not the picture
    let bob_chat = bob.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &expected_title);

    // Bob runs the outbound service: this executes FetchGroupProfileOperation,
    // downloading and decrypting the encrypted group profile from object storage
    bob.outbound_service().run_once().await;

    // Bob should have the correct group attributes after fetching the encrypted profile
    let bob_chat = bob.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &expected_title);
    assert_eq!(bob_chat.attributes().unwrap().picture, expected_picture);

    // Regression: joining the group should not surface spurious "title changed" or "picture
    // changed" system messages: the title is unchanged and the picture is just the initial
    // download.
    let mut bob_messages = bob.messages(chat_id, 1024).await.unwrap();
    // Only keep system messages about the title or picture change
    bob_messages.retain(|message| {
        matches!(
            message.message(),
            Message::Event(EventMessage::System(
                SystemMessage::ChangeTitle { .. } | SystemMessage::ChangePicture(_),
            ))
        )
    });
    assert!(
        bob_messages.is_empty(),
        "unexpected system messages on initial join: {bob_messages:#?}"
    );
}

/// Tests that after a group title and picture update, other members fetch the new encrypted group
/// profile via the outbound service and see the updated attributes.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn member_fetches_updated_group_profile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let chat = alice.expect_chat(chat_id).await;
    let title = chat.attributes().unwrap().title().to_owned();

    // Bob sees the group title of group immediately
    let bob_chat = bob.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &title);

    // Alice updates the group title and picture
    let new_title = "Updated Group Title".to_string();
    alice
        .set_chat_title(chat_id, new_title.clone())
        .await
        .unwrap();
    alice
        .set_chat_picture(chat_id, Some(test_picture_bytes()))
        .await
        .unwrap();

    // Record Alice's stored picture (may be resized relative to test_picture_bytes)
    let chat = alice.expect_chat(chat_id).await;
    let expected_picture = chat.attributes().unwrap().picture.clone();

    // Bob fetches Alice's commits: this schedules FetchGroupProfileOperations
    bob.drain().await;

    // Bob runs the outbound service: this executes FetchGroupProfileOperations,
    // downloading and decrypting the new encrypted group profile from object storage
    bob.outbound_service().run_once().await;

    // Bob should see the updated title and picture
    let bob_chat = bob.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &new_title);
    assert_eq!(
        bob_chat.attributes().unwrap().picture(),
        expected_picture.as_deref()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn profile_component_group_syncs_title_and_picture() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup
        .create_group_with_profile_component(&alice, setup.apq_groups)
        .await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    for user in [&alice, &bob] {
        assert!(user.has_group_profile_component(chat_id).await.unwrap());
        // The profile is read from the component, not from the group data extension.
        let group_data = user.group_data(chat_id).await.unwrap().unwrap();
        assert!(group_data.encrypted_title.is_some());
    }

    // Alice updates the title
    let title = "Component Title".to_string();
    alice.set_chat_title(chat_id, title.clone()).await.unwrap();
    assert_eq!(
        alice
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title(),
        &title
    );
    assert!(alice.has_group_profile_component(chat_id).await.unwrap());

    // Bob sees the new title
    bob.drain().await;
    assert_eq!(
        bob.expect_chat(chat_id).await.attributes().unwrap().title(),
        &title
    );

    // Bob updates the picture
    let picture = test_picture_bytes();
    bob.set_chat_picture(chat_id, Some(picture)).await.unwrap();
    let expected_picture = bob
        .expect_chat(chat_id)
        .await
        .attributes()
        .unwrap()
        .picture()
        .unwrap()
        .to_owned();

    // Alice fetches the new external group profile
    alice.drain().await;
    alice.outbound_service().run_once().await;
    let alice_chat = alice.expect_chat(chat_id).await;
    assert_eq!(alice_chat.attributes().unwrap().title(), &title);
    assert_eq!(
        alice_chat.attributes().unwrap().picture(),
        Some(expected_picture.as_slice())
    );
}
