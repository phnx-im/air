// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Linking new devices: the linking session, its error cases, device names and
//! resulting client state.

use std::{assert_matches, collections::HashSet, time::Duration};

use aircoreclient::{
    ChatId, ChatStatus, UserProfile,
    clients::{
        CoreUser, ListenResponse, listen_response,
        multi_device::{MultiDeviceLinkClientError, MultiDeviceProvisionStep},
        store::ClientRecord,
    },
};
use airprotos::queue_service::v1::{SiblingClientState, sibling_client_state};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{RelaySettings, TestBackend, TestBackendParams},
};
use tempfile::TempDir;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    auto_confirm, client_db_files, device_linked_client_ids, ignore_connected,
    next_sibling_client_state, recv_linking_code, self_chat_id, self_group_client_ids,
    send_and_receive,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_inherits_groups_and_stays_in_sync() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    // Alice already has two groups before the new device is linked. The
    // linked device should inherit them, not just the self-group.
    let chat_id_1 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_1, &alice, &[&bob]).await;

    let chat_id_2 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_2, &alice, &[&bob]).await;

    let alice2 = alice.add_linked_device().await;

    assert_eq!(
        alice2.qs_user_id(),
        alice.qs_user_id(),
        "linked device must share the virtual client (QsUserId)"
    );
    assert_ne!(
        alice2.qs_client_id(),
        alice.qs_client_id(),
        "linked device must have its own queue (QsClientId)"
    );
    let old_device_self_group = alice
        .self_group()
        .await
        .unwrap()
        .expect("old device should have a self group");
    let new_device_self_group = alice2
        .self_group()
        .await
        .unwrap()
        .expect("new device should have a self group");
    assert_eq!(
        old_device_self_group.group_id(),
        new_device_self_group.group_id(),
        "linked device must know the shared self group"
    );

    assert_eq!(
        alice.self_group_member_count().await.unwrap(),
        Some(2),
        "old device should see both emulator clients in the self group"
    );
    assert_eq!(
        alice2.self_group_member_count().await.unwrap(),
        Some(2),
        "new device should see both emulator clients in the self group"
    );

    // The self group carries per-device SelfGroupCredentials, not user
    // credentials. Both devices must see the same pair of distinct client ids,
    // and that pair must match the client id each device stored locally.
    let old_device_ids = self_group_client_ids(&alice).await;
    let new_device_ids = self_group_client_ids(&alice2).await;
    assert_ne!(
        old_device_ids[0], old_device_ids[1],
        "the two self-group leaves must have distinct client ids"
    );
    let old_set: HashSet<Uuid> = old_device_ids.into_iter().collect();
    let new_set: HashSet<Uuid> = new_device_ids.into_iter().collect();
    assert_eq!(
        old_set, new_set,
        "both devices must see the same set of self-group client ids"
    );
    let expected: HashSet<Uuid> = [
        alice.own_client_id().await.unwrap(),
        alice2.own_client_id().await.unwrap(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        old_set, expected,
        "self-group client ids must match each device's own client id"
    );

    assert!(
        alice.self_chat_id().await.unwrap().is_some(),
        "old device should have a self-chat"
    );
    assert!(
        alice2.self_chat_id().await.unwrap().is_some(),
        "new device should have a Notes to self chat"
    );

    // Onboarding into the pre-existing groups is queued and processed in the background.
    alice2.outbound_service().run_once().await;

    // The new device must know about all groups from the original client.
    let new_device_chat_ids = alice2.ordered_chat_ids().await.unwrap();
    for (label, chat_id) in [("1", chat_id_1), ("2", chat_id_2)] {
        assert!(
            new_device_chat_ids.contains(&chat_id),
            "linked device should have inherited pre-existing group {label}"
        );
        assert!(
            alice2.resync_status(chat_id).await.unwrap().is_none(),
            "onboarding into group {label} should have completed, not still be queued"
        );

        let members = alice2.mls_chat_participants(chat_id).await;
        assert!(
            members
                .as_ref()
                .is_some_and(|members| members.contains(alice.user_id())),
            "chat {label} on the linked device should be bound to a joined MLS group, got {members:?}"
        );
        assert_eq!(
            members,
            alice.mls_chat_participants(chat_id).await,
            "linked device should see the same members as the old device in group {label}"
        );
    }

    // Linking the new device is recorded in the self group, not in each
    // pre-existing chat: the old device's commit adding the new device's
    // self-group leaf must produce a DeviceLinked system message identifying
    // it, not a generic Add (which would render as e.g. "Alice added Alice").
    let self_group_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    assert_eq!(
        device_linked_client_ids(&alice, self_group_chat_id).await,
        vec![alice2.own_client_id().await.unwrap()],
        "linking the new device should record a DeviceLinked system message in the self group"
    );

    // Messages sent into the self group are seen by the other device.
    let self_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    send_and_receive(
        &alice,
        &[&alice2],
        self_chat_id,
        "hello from the old device in self-group",
    )
    .await;
    send_and_receive(
        &alice2,
        &[&alice],
        self_chat_id,
        "hello back from the new device in self-group",
    )
    .await;

    // The old device has to follow the onboarding external commit onto the virtual client's new leaf.
    alice.drain().await;
    assert_eq!(
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must land on the same epoch and shared leaf"
    );

    // Messages sent into one of the existing groups are seen by both clients.
    for chat_id in [chat_id_1, chat_id_2] {
        send_and_receive(&alice, &[&alice2], chat_id, "hello from the old device").await;
        send_and_receive(
            &alice2,
            &[&alice],
            chat_id,
            "hello back from the new device",
        )
        .await;
    }

    // Messages received from a 3rd party are seen by both clients.
    send_and_receive(
        &bob,
        &[&alice2, &alice],
        chat_id_1,
        "hello from the old device",
    )
    .await;

    // A commit that replaces the shared leaf for some other reason
    // has to derive the new leaf from the emulation epoch too,
    // or the sibling emulator client will break.
    setup.invite_to_group(chat_id_1, &alice, &[&charlie]).await;

    alice2.drain().await;
    assert_eq!(
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after an invite+add"
    );

    send_and_receive(&bob, &[&alice2], chat_id_1, "after a member was added").await;

    // Rotating the user profile updates the profile key on the self group. That
    // DS request envelope is now signed with the per-device self-group key, so
    // this confirms the flipped self-group credential still authenticates
    // self-group operations.
    let new_profile = UserProfile {
        user_id: alice.user_id().clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };
    alice.set_own_user_profile(new_profile).await.unwrap();

    // Same for a removal.
    setup
        .remove_from_group(chat_id_1, &alice, &[&charlie])
        .await
        .unwrap();

    alice2.drain().await;
    assert_eq!(
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after a removal"
    );

    send_and_receive(&bob, &[&alice2], chat_id_1, "after a member was removed").await;

    // And for a deletion, which replaces the shared leaf as well. Without
    // following it the linked device never learns the group is gone.
    setup.delete_group(chat_id_1, &alice).await;

    alice2.drain().await;
    let deleted_chat = alice2
        .chat(&chat_id_1)
        .await
        .expect("linked device should still know the deleted chat");
    assert!(
        matches!(deleted_chat.status(), ChatStatus::Inactive(_)),
        "linked device should see the deleted group as inactive, got {:?}",
        deleted_chat.status()
    );
}

// Linking with a session ID that was never registered returns an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn link_with_unknown_session_fails() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // A well-formed code for a session that was never opened.
    let result = alice
        .multi_device_link_client("999".to_owned(), ignore_connected(), auto_confirm())
        .await;

    assert!(matches!(
        result,
        Ok(Err(MultiDeviceLinkClientError::SessionNotFound))
    ));
}

// A session can only be claimed once; a second link attempt on the same session ID
// must fail even when called by the same user.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn consumed_session_cannot_be_linked_again() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(domain, server_url, session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let session_id = recv_linking_code(&mut session_rx).await;

    alice
        .multi_device_link_client(session_id.clone(), ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    new_device_task.await.unwrap();

    // Session was already consumed — a second attempt must fail.
    let second_result = alice
        .multi_device_link_client(session_id, ignore_connected(), auto_confirm())
        .await;

    assert!(matches!(
        second_result,
        Ok(Err(MultiDeviceLinkClientError::SessionNotFound))
    ));
}

// Two concurrent linking sessions must not interfere with each other.
// Each new device must be linked to the correct existing device.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn concurrent_linking_sessions_stay_separate() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let [alice, bob] = setup.add_users().await;

    let (alice_session_tx, mut alice_session_rx) = tokio::sync::mpsc::channel(1);
    let (bob_session_tx, mut bob_session_rx) = tokio::sync::mpsc::channel(1);

    let alice_domain = domain.clone();
    let alice_server_url = server_url.clone();
    let alice2_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(alice_domain, alice_server_url, alice_session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let bob_domain = domain.clone();
    let bob_server_url = server_url.clone();
    let bob2_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(bob_domain, bob_server_url, bob_session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let alice_session_id = recv_linking_code(&mut alice_session_rx).await;
    let bob_session_id = recv_linking_code(&mut bob_session_rx).await;

    // Concurrent sessions must get distinct rendezvous ids, and therefore
    // distinct codes.
    assert_ne!(alice_session_id, bob_session_id);

    alice
        .multi_device_link_client(alice_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    bob.multi_device_link_client(bob_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    // Each new device must be linked to the correct existing virtual client.
    let alice2 = alice2_task.await.unwrap();
    let bob2 = bob2_task.await.unwrap();
    assert_eq!(alice2.qs_user_id(), alice.qs_user_id());
    assert_eq!(bob2.qs_user_id(), bob.qs_user_id());
    assert_ne!(alice2.qs_user_id(), bob2.qs_user_id());
}

// The new device's metadata entry rides on the self-group add commit itself, so
// linking costs exactly one commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn link_publishes_device_entry_without_extra_commit() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let old_id = alice.own_client_id().await.unwrap();
    let alice2 = alice.add_linked_device().await;
    let new_id = alice2.own_client_id().await.unwrap();

    // No outbound run and no queue drain: both sides already hold both entries.
    // The old device folded the new entry in while staging the add, and the new
    // device wrote its own entry before handing a copy over.
    for (label, device) in [("old", &alice), ("new", &alice2)] {
        let ids: Vec<_> = device
            .linked_devices()
            .await
            .unwrap()
            .into_iter()
            .map(|device| device.client_id)
            .collect();
        assert!(
            ids.contains(&old_id) && ids.contains(&new_id),
            "the {label} device should know both entries right after linking, got {ids:?}"
        );
    }

    // Neither side owes a settings commit, so the self group stays at the epoch
    // the add left it on.
    for (label, device) in [("old", &alice), ("new", &alice2)] {
        assert!(
            !device.has_pending_setting_changes().await.unwrap(),
            "the {label} device must not have queued a settings commit for the entry"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn third_device_onboards_without_duplicate_fanout() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    // A higher-level group Alice is already a member of. Every device linked
    // from here on onboards into it with a resync, and unlike the self group its
    // commits are broadcast to all of Alice's emulator queues. The connection
    // group with bob is a second such group.
    let group_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(group_chat_id, &alice, &[&bob]).await;

    let alice2 = alice.add_linked_device().await;
    // Device 2's onboarding replaces Alice's own, not-yet-virtual leaf, and
    // turns it into a virtual-client leaf.
    alice2.outbound_service().run_once().await;

    let alice3 = alice.add_linked_device().await;

    // Empty every queue, so what arrives next is exactly the fan-out of device
    // 3's onboarding commit: the resync is only enqueued at this point and runs
    // when device 3's outbound service is driven below.
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    for (label, device) in [("1", &alice), ("2", &alice2), ("3", &alice3)] {
        assert_eq!(
            device.self_group_member_count().await.unwrap(),
            Some(3),
            "device {label} should see all three emulator clients in the self group"
        );
    }

    // Device 3 onboards into both higher-level groups. The leaf each onboarding
    // replaces is already a virtual-client leaf, so the sibling queue is covered
    // by the regular destination list; neither commit must be fanned out twice.
    // The same outbound run also stages device 3's key packages via a self-group
    // commit, so the siblings see exactly three commits, each delivered once.
    alice3.outbound_service().run_once().await;
    for (label, device) in [("1", &alice), ("2", &alice2)] {
        let queued = device.qs_fetch_messages().await.unwrap();
        assert_eq!(
            queued.len(),
            3,
            "device {label} should receive device 3's key-package upload and its two \
             onboarding commits exactly once each"
        );
        let processed = device.fully_process_qs_messages(queued).await;
        assert!(
            processed.errors.is_empty(),
            "device {label} failed to process device 3's commits: {:?}",
            processed.errors
        );
    }

    for (chat_label, onboarded_chat_id) in [
        ("higher-level group", group_chat_id),
        ("connection group", connection_chat_id),
    ] {
        assert!(
            alice3
                .resync_status(onboarded_chat_id)
                .await
                .unwrap()
                .is_none(),
            "device 3 should have completed onboarding into the {chat_label}"
        );
        let epoch_and_index = alice
            .group_epoch_and_own_index(onboarded_chat_id)
            .await
            .unwrap();
        for (label, device) in [("2", &alice2), ("3", &alice3)] {
            assert_eq!(
                device
                    .group_epoch_and_own_index(onboarded_chat_id)
                    .await
                    .unwrap(),
                epoch_and_index,
                "device {label} should share the virtual client's leaf and epoch \
                 in the {chat_label}"
            );
        }
    }

    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice, &[&alice2, &alice3], chat_id, "from device 1").await;
    send_and_receive(&alice3, &[&alice, &alice2], chat_id, "from device 3").await;

    // The higher-level group is usable from the newest device.
    send_and_receive(
        &alice3,
        &[&alice, &alice2],
        group_chat_id,
        "from device 3 in the group",
    )
    .await;
}

// Linking a third device fans the add commit out to the existing non-initiating
// device, which must process the self-group add without desynchronizing.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn third_device_link_keeps_second_device_in_sync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let alice2 = alice.add_linked_device().await;
    // Bring the second device fully up to date before the third link.
    alice2.drain().await;

    let alice3 = alice.add_linked_device().await;

    // The second device processes the add commit for the third device.
    alice2.drain().await;

    // All three devices agree on the membership.
    alice.drain().await;
    for (device, name) in [
        (&alice, "old device"),
        (&alice2, "second device"),
        (&alice3, "third device"),
    ] {
        assert_eq!(
            device.self_group_member_count().await.unwrap(),
            Some(3),
            "{name} should see all three emulator clients in the self group"
        );
    }

    // The second device is still in sync: a message from the old device
    // round-trips.
    let chat_id = self_chat_id(&alice2).await;
    send_and_receive(&alice, &[&alice2], chat_id, "three devices").await;
}

/// After linking, each device advertises itself and learns its sibling, so both
/// see the same two-device list. This is the property the Devices screen shows.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_lists_converge() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    assert_ne!(a_id, b_id, "each device mints its own client id");

    // Neither entry needs a round trip of its own. The old device's entry
    // predates the self group, so it never became a pending change and rode
    // along in the provisioning snapshot. The new device's entry travelled the
    // other way in the join request, and the old device folded it into the add
    // commit itself, which is why nothing is left enqueued on either side (see
    // `link_publishes_device_entry_without_extra_commit`).
    for user in [&alice, &alice2] {
        let members = user.self_group_client_ids().await?;
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a_id) && members.contains(&b_id));

        let devices = user.linked_devices().await?;
        let ids: Vec<_> = devices.iter().map(|d| d.client_id).collect();
        assert!(
            ids.contains(&a_id) && ids.contains(&b_id),
            "both devices must be in the synced metadata, got {ids:?}"
        );
    }

    Ok(())
}

/// The name the confirming user typed overrides the name the new device chose
/// for itself, and reaches both devices.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn link_uses_confirmed_device_name() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice
        .link_ephemeral_device_named("Work laptop")
        .await
        .unwrap();

    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    for (label, device) in [("old", &alice), ("new", &alice2)] {
        let seen = device
            .linked_devices()
            .await?
            .into_iter()
            .find(|device| device.client_id == b_id)
            .unwrap_or_else(|| panic!("the {label} device should know the new device"));
        assert_eq!(
            seen.name, "Work laptop",
            "the {label} device should use the confirmed name"
        );
    }

    Ok(())
}

/// A blank confirmation name leaves the platform default the new device picked.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn link_blank_name_keeps_device_default() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.link_ephemeral_device_named("   ").await.unwrap();

    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    let seen = alice
        .linked_devices()
        .await?
        .into_iter()
        .find(|device| device.client_id == b_id)
        .expect("the old device should know the new device");
    assert!(
        !seen.name.trim().is_empty(),
        "a blank confirmation must not blank out the device name"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn cancelled_provisioning_leaves_no_client() -> anyhow::Result<()> {
    let setup = TestBackend::single().await;
    let tmp = TempDir::new()?;
    let db_path = tmp.path().to_str().unwrap().to_owned();

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let domain = setup.domain().clone();
        let server_url = setup.server_url();
        let db_path = db_path.clone();
        let cancel = cancel.clone();
        async move { CoreUser::new_linked(&db_path, domain, Some(server_url), session_tx, cancel).await }
    });
    recv_linking_code(&mut session_rx).await;

    cancel.cancel();
    assert!(task.await?.is_err(), "cancelled provisioning must fail");

    assert!(
        ClientRecord::load_all_from_air_db(&db_path)
            .await?
            .is_empty()
    );
    assert!(
        client_db_files(tmp.path()).is_empty(),
        "no client DB files may remain: {:?}",
        client_db_files(tmp.path())
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_client_state_is_relayed() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    let (mut old_stream, old_responder) = alice.listen_queue().await.unwrap();
    let (mut new_stream, _new_responder) = alice2.listen_queue().await.unwrap();

    old_responder.report_client_state(b"state".to_vec()).await;
    let sibling_client_state::Change::Updated(updated) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected updated");
    };
    assert_eq!(updated.blob.as_ref().unwrap().encrypted_blob, b"state");
    let old_client_id = updated.client_id.unwrap();

    // A new session gets the current state right after the version status.
    let (mut new_stream, _new_responder) = alice2.listen_queue().await.unwrap();
    assert_matches!(
        new_stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::VersionStatus(_)),
        }))
    );
    assert_matches!(
        new_stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::SiblingClientState(SiblingClientState {
                change: Some(sibling_client_state::Change::Updated(snapshot)),
            })),
        })) if snapshot.client_id == updated.client_id
            && snapshot.epoch == updated.epoch
            && snapshot.blob == updated.blob
    );

    // Ending the session clears the state.
    old_responder.close(&mut old_stream).await;
    let sibling_client_state::Change::Removed(removed) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected removed");
    };
    assert_eq!(removed.client_id, Some(old_client_id));
    assert!(removed.epoch > updated.epoch);

    // Oversized states are ignored, the session goes on.
    let (_old_stream, old_responder) = alice.listen_queue().await.unwrap();
    old_responder.report_client_state(vec![0; 1024]).await;
    old_responder.report_client_state(b"after".to_vec()).await;
    let sibling_client_state::Change::Updated(updated) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected updated");
    };
    assert_eq!(updated.blob.unwrap().encrypted_blob, b"after");
}

/// Spawns a provisioning device and hands back its linking code together
/// with the task.
async fn provision_device(
    setup: &TestBackend,
) -> (
    String,
    tokio::task::JoinHandle<anyhow::Result<CoreUser>>,
    tokio::sync::mpsc::Receiver<MultiDeviceProvisionStep>,
) {
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let task = tokio::spawn(async move {
        anyhow::Ok(CoreUser::new_linked_ephemeral(domain, server_url, session_tx).await??)
    });

    let code = recv_linking_code(&mut session_rx).await;
    (code, task, session_rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linking_session_expires() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        relay: RelaySettings {
            sessionttl: Duration::from_millis(300),
            ..RelaySettings::default()
        },
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;

    let (code, new_device_task, _session_rx) = provision_device(&setup).await;

    assert!(
        new_device_task.await.unwrap().is_err(),
        "the new device's session must end when the relay reaps it"
    );

    let result = alice
        .multi_device_link_client(code, ignore_connected(), auto_confirm())
        .await;
    assert!(matches!(
        result,
        Ok(Err(MultiDeviceLinkClientError::SessionNotFound))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn rendezvous_id_is_not_reused() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let (first_code, first_task, _first_rx) = provision_device(&setup).await;
    alice
        .multi_device_link_client(first_code.clone(), ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();
    let _first_device = first_task.await.unwrap().unwrap();

    let (second_code, second_task, _second_rx) = provision_device(&setup).await;
    assert_ne!(
        second_code, first_code,
        "an ended rendezvous id must stay in quarantine"
    );
    second_task.abort();
}
