// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Open incoming and outgoing requests on a newly linked device.

use aircoreclient::{ChatType, SystemMessage};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::{has_system_message, request_expires_at};
use crate::multi_device::send_and_receive;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_gets_open_incoming_requests() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie, dave] = setup.add_users().await;

    // Alice and charlie share a group, through dave.
    let group_chat_id = setup.connected_group(&dave, &[&alice, &charlie]).await;
    let record = setup.add_username(&alice).await;

    // An open request via alice's username from bob, and one through the group
    // from charlie.
    setup
        .user(&bob)
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let username_chat_id = alice.receive_connection_offer(&record).await;
    setup
        .user(&charlie)
        .add_contact_from_group(group_chat_id, alice.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();
    let processed = alice.drain().await;
    let [group_request_chat_id] = processed.new_connections.as_slice() else {
        panic!("expected one request, got {:?}", processed.new_connections);
    };

    let alice2 = alice.add_synced_device().await;

    let username_chat = alice2.expect_chat(username_chat_id).await;
    assert_eq!(
        username_chat.chat_type(),
        &ChatType::PendingConnection(bob.user_id().clone())
    );
    assert!(
        has_system_message(&alice2, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == bob.user_id() && user_handle == &record.username
        ))
        .await
    );
    let group_request_chat = alice2.chat(group_request_chat_id).await.unwrap();
    assert_eq!(
        group_request_chat.chat_type(),
        &ChatType::PendingConnection(charlie.user_id().clone())
    );
    assert!(
        has_system_message(&alice2, *group_request_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedGroupConnectionRequest { sender, origin_chat_id }
                if sender == charlie.user_id() && origin_chat_id == &group_chat_id
        ))
        .await
    );

    // The new device accepts both requests, which the original device follows.
    for chat_id in [username_chat_id, *group_request_chat_id] {
        alice2
            .accept_contact_request(chat_id)
            .await
            .unwrap()
            .unwrap();
    }
    alice.drain().await;
    assert_eq!(
        alice.expect_chat(username_chat_id).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert_eq!(
        alice.chat(group_request_chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(charlie.user_id().clone())
    );
}

/// A new device gets the open outgoing requests of its user and follows the
/// recipients accepting them. When they accept before it onboards, the new
/// device holds their joins only in its queue, at an epoch before the one it
/// onboards at.
async fn linked_device_gets_open_outgoing_requests(acceptance: Acceptance) {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie, dave] = setup.add_users().await;

    // Alice and charlie share a group, through dave.
    let group_chat_id = setup.connected_group(&dave, &[&alice, &charlie]).await;
    let record = setup.add_username(&bob).await;

    // An open request to bob's username, and one through the group to charlie.
    let username_chat_id = alice
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let group_request_chat_id = alice
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    if let Acceptance::AfterOnboarding = acceptance {
        alice2.outbound_service().run_once().await;
        alice.drain().await;
        alice2.drain().await;
        assert_eq!(
            alice2.expect_chat(username_chat_id).await.chat_type(),
            &ChatType::HandleConnection(record.username.clone())
        );
        assert_eq!(
            alice2.expect_chat(group_request_chat_id).await.chat_type(),
            &ChatType::TargetedMessageConnection(charlie.user_id().clone())
        );
        // The requests expire at the same time on both devices.
        for chat_id in [username_chat_id, group_request_chat_id] {
            let expires_at = request_expires_at(&alice, chat_id).await;
            assert!(expires_at.is_some());
            assert_eq!(request_expires_at(&alice2, chat_id).await, expires_at);
        }
    }

    bob.receive_connection_offer(&record).await;
    bob.accept_contact_request(username_chat_id)
        .await
        .unwrap()
        .unwrap();
    charlie.drain().await;
    charlie
        .accept_contact_request(group_request_chat_id)
        .await
        .unwrap()
        .unwrap();

    match acceptance {
        Acceptance::AfterOnboarding => {}
        Acceptance::BeforeOnboardingJoinsFirst => {
            alice2.drain().await;
            alice2.outbound_service().run_once().await;
        }
        Acceptance::BeforeOnboardingJoinsLast => {
            alice2.outbound_service().run_once().await;
            alice2.drain().await;
        }
    }

    for (label, device) in [("original", &alice), ("new", &alice2)] {
        device.drain().await;
        assert_eq!(
            device.expect_chat(username_chat_id).await.chat_type(),
            &ChatType::Connection(bob.user_id().clone()),
            "the {label} device should see the connection to bob"
        );
        assert_eq!(
            device.expect_chat(group_request_chat_id).await.chat_type(),
            &ChatType::Connection(charlie.user_id().clone()),
            "the {label} device should see the connection to charlie"
        );
    }
    assert!(
        has_system_message(&alice2, group_request_chat_id, |message| matches!(
            message,
            SystemMessage::SentGroupConnectionRequest { recipient, origin_chat_id }
                if recipient == charlie.user_id() && origin_chat_id == &group_chat_id
        ))
        .await
    );
    assert!(
        has_system_message(&alice2, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedConnectionConfirmation { sender, .. } if sender == bob.user_id()
        ))
        .await
    );

    send_and_receive(&alice2, &[&bob, &alice], username_chat_id, "hi bob").await;
    send_and_receive(
        &alice2,
        &[&charlie, &alice],
        group_request_chat_id,
        "hi charlie",
    )
    .await;

    // A commit of the new device passes the room state checks of the others.
    alice2.update_key(username_chat_id).await.unwrap();
    send_and_receive(&bob, &[&alice, &alice2], username_chat_id, "hi alice").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_after_onboarding() {
    linked_device_gets_open_outgoing_requests(Acceptance::AfterOnboarding).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_before_onboarding_joins_first() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsFirst).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_before_onboarding_joins_last() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsLast).await;
}

/// When the recipients accept the outgoing requests, relative to the new
/// device's onboarding into their connection groups.
#[derive(Debug, Clone, Copy)]
enum Acceptance {
    AfterOnboarding,
    /// Before the onboarding, with the new device processing the joins before
    /// it onboards.
    BeforeOnboardingJoinsFirst,
    /// Before the onboarding, with the new device processing the joins after
    /// it onboards.
    BeforeOnboardingJoinsLast,
}
