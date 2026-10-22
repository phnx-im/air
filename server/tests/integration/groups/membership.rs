// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Inviting, removing, re-adding and leaving members, and the profiles and key
//! packages involved.

use std::{collections::HashSet, slice};

use aircoreclient::{
    ChatStatus, DisplayName, UserProfile,
    clients::MarkChatAsRead,
    outbound_service::{APQ_KEY_PACKAGES, KEY_PACKAGES},
};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams, expected_display_name},
};
use chrono::Utc;
use mimi_content::MimiContent;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invite_to_group() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connected_group(&alice, &[&bob, &charlie]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invite_to_group_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connected_apq_group(&alice, &[&bob, &charlie]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "a duplicate invitee is not deduplicated"]
async fn duplicate_invitee_is_added_once() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    alice.drain().await;

    let bob_id = bob.user_id().clone();
    let result = alice
        .invite_users(chat_id, &[bob_id.clone(), bob_id])
        .await
        .unwrap();
    assert!(result.users_not_added.is_empty());
    bob.drain().await;

    for user in [&alice, &bob] {
        let members = user.mls_members(chat_id).await.unwrap().unwrap();
        assert_eq!(members.len(), 2);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn removed_member_profile_resets_to_default() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie, dave] = setup.add_users().await;

    let chat_id = setup
        .connected_group(&alice, &[&bob, &charlie, &dave])
        .await;
    // Check that Charlie has a user profile stored for bob, even though
    // he hasn't connected with them.
    let charlie_user_profile_bob = charlie.user_profile(bob.user_id()).await;
    assert!(charlie_user_profile_bob.user_id == *bob.user_id());

    setup
        .remove_from_group(chat_id, &alice, &[&bob])
        .await
        .unwrap();

    // Now that charlie is not in a group with Bob anymore, the user profile
    // should be the default one derived from the client id.
    let charlie_user_profile_bob = charlie.user_profile(bob.user_id()).await;
    assert_eq!(
        charlie_user_profile_bob,
        UserProfile::from_user_id(bob.user_id())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn member_can_be_readded_repeatedly() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    for _ in 0..10 {
        setup
            .remove_from_group(chat_id, &alice, &[&bob])
            .await
            .unwrap();
        setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    }
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_group() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    setup.leave_group(chat_id, &bob).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn left_member_receives_no_messages() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Bob leaves and drops the group, but the DS fans out group messages to
    // him until somebody commits his removal.
    bob.leave_chat(chat_id).await.unwrap();
    assert!(bob.mls_chat_participants(chat_id).await.is_none());
    let bob_messages_before = bob.messages(chat_id, 1024).await.unwrap();

    let content = MimiContent::simple_markdown_message("Hello".to_owned(), [0; 16]);
    alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();

    charlie.leave_chat(chat_id).await.unwrap();

    let result = bob.drain_unchecked().await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        result.new_messages.is_empty(),
        "Bob should not get messages after leaving: {:?}",
        result.new_messages
    );
    assert_eq!(
        bob.messages(chat_id, 1024).await.unwrap(),
        bob_messages_before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_leave_removes_member_on_both_legs() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_apq_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    // Alice leaves the APQ group.
    alice.leave_chat(chat_id).await.unwrap();

    // Bob picks up Alice's self-remove proposal.
    bob.drain().await;

    let debug_info = bob.chat_debug_info(chat_id).await.unwrap();
    let pq = debug_info
        .pq
        .clone()
        .expect("APQ group must have a PQ group");

    assert_eq!(
        debug_info.pending_proposals, 1,
        "T group must have Alice's self-remove proposal staged"
    );
    assert_eq!(
        pq.pending_proposals, 1,
        "PQ group must also have Alice's self-remove proposal staged"
    );
    let pq_epoch_before = pq.epoch;

    // A non-leaver (Bob) commits the self-remove via an APQ FULL commit, which
    // sweeps the pending proposal on both the T and PQ groups.
    bob.update_apq_key(chat_id).await.unwrap();

    // Charlie picks up the commit ...
    charlie.drain().await;

    // ... and Alice learns she was removed.
    alice.drain().await;

    // Both Bob and Charlie must have committed the removal on the T and PQ
    // groups: Alice is gone, nothing is left pending, and the PQ epoch advanced.
    for (label, user) in [("Bob", &bob), ("Charlie", &charlie)] {
        let debug_info = user.chat_debug_info(chat_id).await.unwrap();
        let pq = debug_info.pq.expect("APQ group must have a PQ group");

        assert_eq!(
            debug_info.members.len(),
            2,
            "{label}'s T group must have Alice removed (only Bob and Charlie left)"
        );
        assert_eq!(
            debug_info.pending_proposals, 0,
            "{label}'s T group must have no pending proposals after the commit"
        );
        assert_eq!(
            pq.pending_proposals, 0,
            "{label}'s PQ group must have no pending proposals after the commit"
        );
        assert!(
            pq.epoch > pq_epoch_before,
            "{label}'s PQ group must advance its epoch by committing the removal"
        );
    }

    // Alice's chat is now inactive.
    let alice_chat = alice.expect_chat(chat_id).await;
    assert!(
        matches!(alice_chat.status(), ChatStatus::Inactive(_)),
        "Alice's chat must be inactive after being removed from the APQ group"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn joining_member_publishes_updated_profile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Alice and Bob are connected.
    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    // Bob and Charlie are connected.
    let _bob_charlie_chat = setup.connect_users(&bob, &charlie).await;

    // Alice updates her profile.
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();
    let alice_profile = UserProfile {
        user_id: alice.user_id().clone(),
        display_name: alice_display_name.clone(),
        profile_picture: None,
    };
    alice.set_own_user_profile(alice_profile).await.unwrap();

    // Bob doesn't fetch his queue, so he doesn't know about Alice's new profile.
    // He creates a group and invites Charlie.
    let chat_id = setup.create_group(&bob).await;

    bob.invite_users(chat_id, slice::from_ref(charlie.user_id()))
        .await
        .unwrap();

    // Charlie accepts the invitation.
    charlie.drain().await;

    // Bob now invites Alice
    bob.invite_users(chat_id, slice::from_ref(alice.user_id()))
        .await
        .unwrap();

    // Charlie processes his messages again, fetching Alice's profile will fail because it tries to
    // download Alice's old profile.
    charlie.qs_fetch_messages().await.unwrap();
    charlie.outbound_service().run_once().await;

    let alice_profile = charlie.user_profile(alice.user_id()).await;
    assert_eq!(
        alice_profile,
        UserProfile::from_user_id(alice.user_id()),
        "Fetching Alice's profile should have failed"
    );

    // Alice accepts the invitation.
    alice.drain().await;

    // While processing her messages, Alice should have issued a profile update

    // Charlie picks up his messages.
    charlie.drain().await;
    charlie.outbound_service().run_once().await;

    // Charlie should now have Alice's new profile.
    let charlie_user_profile = charlie.user_profile(alice.user_id()).await;
    assert_eq!(charlie_user_profile.display_name, alice_display_name);
}

/// A member joining from a Welcome that is several epochs stale.
///
/// `ds_welcome_info` answers for the epoch asked for, so the ratchet tree it
/// returns is historical. The profile keys shipped with it have to describe
/// that same epoch. Serving the group's current keys instead means any leaf
/// that changed hands in between resolves to the wrong occupant, and the
/// ciphertext is bound to a user id, so it does not decrypt.
///
/// The joiner is deliberately stopped after its Welcome and before it catches
/// up: later commits carry added members' profile keys in their AAD, which
/// would re-deliver the keys and hide the problem.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stale_welcome_keeps_profile_keys() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await; // committer
    let bob = setup.add_user().await; // joins from a stale Welcome
    let charlie = setup.add_user().await; // leaves the group, freeing his leaf
    let dave = setup.add_user().await; // stays throughout
    let eve = setup.add_user().await; // takes over Charlie's leaf

    for member in [&bob, &charlie, &dave, &eve] {
        setup.connect_users(&alice, member).await;
    }
    for member in [&alice, &bob, &charlie, &dave, &eve] {
        let profile = UserProfile {
            user_id: member.user_id().clone(),
            display_name: expected_display_name(member.user_id()),
            profile_picture: None,
        };
        member.set_own_user_profile(profile).await.unwrap();
    }

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_and_settle(&alice, chat_id, &[&charlie, &dave])
        .await;
    let charlie_index = setup.own_leaf_index(&charlie, chat_id).await;

    // Alice invites Bob. Bob's Welcome describes this epoch, with Charlie
    // still in the group. Bob does not touch his queue yet.
    alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .unwrap();
    setup.settle(&[&alice, &charlie, &dave]).await;

    // The group moves on: Charlie's leaf is vacated and handed to Eve.
    alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .unwrap();
    setup.settle(&[&alice, &dave]).await;
    setup.invite_and_settle(&alice, chat_id, &[&eve]).await;
    let eve_index = setup.own_leaf_index(&eve, chat_id).await;
    assert_eq!(
        eve_index, charlie_index,
        "the test needs Eve to take over Charlie's leaf, otherwise nothing \
         changed hands between Bob's Welcome and now"
    );

    // Bob processes his Welcome and nothing else, so his view is the epoch the
    // Welcome describes rather than the group's current one.
    let queued = bob.qs_fetch_messages().await.unwrap();
    let mut joined = false;
    let mut consumed = 0usize;
    for message in queued {
        bob.fully_process_qs_messages(vec![message]).await;
        consumed += 1;
        if bob.chat_participants(chat_id).await.is_some() {
            joined = true;
            break;
        }
    }
    assert!(
        joined,
        "Bob should have joined after processing his Welcome"
    );
    tracing::info!(consumed, "messages Bob processed before joining");
    // Everything still queued is deliberately left unprocessed.
    for _ in 0..3 {
        bob.outbound_service().run_once().await;
    }

    // Bob is at the Welcome's epoch, where Charlie is still a member.
    let bob_view = bob.chat_participants(chat_id).await.unwrap();
    // Bob's two views of the same group must agree. The ratchet tree is the
    // epoch his Welcome describes; the participant list is derived from the
    // room state that came with it. If the DS answers with a current room
    // state, the list names whoever holds a leaf now rather than who held it
    // at that epoch.
    let mls_view = bob.group_members(chat_id).await.unwrap();
    assert_eq!(
        bob_view, mls_view,
        "Bob's participant list disagrees with his own ratchet tree: the two \
         halves of his welcome info describe different epochs"
    );
    assert!(
        bob_view.contains(charlie.user_id()),
        "Bob should still see Charlie, who was a member at his Welcome's epoch"
    );
    assert!(
        !bob_view.contains(eve.user_id()),
        "Bob should not see Eve, who joined after his Welcome's epoch"
    );

    setup
        .assert_profiles_resolve(&bob, &[&alice, &charlie, &dave], "stale welcome")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn key_packages_fall_back_to_last_resort_then_refresh() {
    let mut setup = TestBackend::single_with_params(TestBackendParams::no_rate_limits()).await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    // Exhaust Bob's key packages
    // We collect Bob's encryption keys. They should be unique every time.
    let mut encryption_keys = HashSet::new();

    let create_chat_and_invite_bob = async |setup: &TestBackend| {
        let chat_id = setup.create_group(&alice).await;
        alice
            .invite_users(chat_id, slice::from_ref(bob.user_id()))
            .await
            .unwrap();
        bob.drain().await;
        bob.mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .find(|m| m.index.usize() == 1)
            .unwrap()
            .encryption_key
    };

    // Number of key packages + 1 for the last resort key package
    let n: usize = if setup.apq_groups {
        APQ_KEY_PACKAGES
    } else {
        KEY_PACKAGES
    } + 1;

    for _ in 0..n {
        let bob_encryption_key = create_chat_and_invite_bob(&setup).await;
        assert!(encryption_keys.insert(bob_encryption_key));
    }

    let bob_encryption_key = create_chat_and_invite_bob(&setup).await;
    assert!(
        !encryption_keys.insert(bob_encryption_key),
        "Alice should have reused Bob's last resort KeyPackage"
    );

    // Bob uploads new KeyPackages
    let now = Utc::now();
    bob.outbound_service()
        .schedule_key_package_upload(now - chrono::Duration::minutes(5))
        .await
        .unwrap();
    bob.outbound_service().run_once().await;

    // Invite Bob again, should get a new KeyPackage
    let bob_encryption_key = create_chat_and_invite_bob(&setup).await;
    assert!(
        n <= 1 || encryption_keys.insert(bob_encryption_key),
        "Bob should have a new KeyPackage after uploading new ones"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "a network error while joining loses the Welcome"]
async fn welcome_survives_network_error() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    alice.drain().await;
    alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .unwrap();

    // Fetching the welcome info fails like a network error, once.
    setup
        .listener_control_handle()
        .set_drop_next_request_to("WelcomeInfo");
    bob.drain_unchecked().await;
    bob.drain().await;

    assert!(
        bob.chat(&chat_id)
            .await
            .is_some_and(|chat| chat.status() == &ChatStatus::Active),
        "bob never joined the group"
    );
}
