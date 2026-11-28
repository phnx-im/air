// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Contact verification marks synced between sibling devices.

use aircommon::identifiers::UserId;
use aircoreclient::{
    ContactVerificationStatus, EventMessage, Message, SystemMessage, clients::CoreUser,
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{DateTime, Utc};

/// The verification records in the chat with the contact, with their times.
async fn verification_records(
    user: &CoreUser,
    contact: &UserId,
) -> Vec<(SystemMessage, DateTime<Utc>)> {
    let chat_id = user.contact(contact).await.expect("no contact").chat_id;
    user.messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .filter_map(|message| match message.message() {
            Message::Event(EventMessage::System(
                record @ (SystemMessage::ContactVerified { .. }
                | SystemMessage::ContactUnverified { .. }),
            )) => Some((record.clone(), message.timestamp())),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn verification_syncs_both_directions() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;
    let bob_id = bob.user_id().clone();
    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Unverified
    );

    alice.set_contact_verified(bob_id.clone(), true).await?;
    assert_eq!(
        alice.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Verified
    );
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Verified
    );

    alice2.set_contact_verified(bob_id.clone(), false).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(
        alice.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Unverified
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_linked_device_starts_with_the_marks() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let bob_id = bob.user_id().clone();
    let charlie_id = charlie.user_id().clone();

    alice.set_contact_verified(bob_id.clone(), true).await?;
    // We send the commit before linking. The new device can only learn the
    // mark from the linking payload.
    alice.outbound_service().run_once().await;

    let alice2 = alice.add_synced_device().await;

    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Verified
    );
    assert_eq!(
        alice2.contact_verification(&charlie_id).await?,
        ContactVerificationStatus::Unverified
    );

    Ok(())
}

// The new device processes the sibling's commit before it onboards into the
// chat with the contact. It learns the credential only then, and the mark
// counts from that point on.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_mark_synced_before_the_credential_counts_once_it_arrives() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let bob_id = bob.user_id().clone();

    let alice2 = alice.add_linked_device().await;
    // The new device's key package commit would make ours stale.
    alice.drain().await;
    alice.set_contact_verified(bob_id.clone(), true).await?;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Unverified,
        "the new device has not pinned the credential yet"
    );

    alice2.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;

    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Verified
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn both_devices_record_each_change_at_the_same_time() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;
    let alice_id = alice.user_id().clone();
    let bob_id = bob.user_id().clone();

    alice.set_contact_verified(bob_id.clone(), true).await?;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    alice2.set_contact_verified(bob_id.clone(), false).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;

    let records = verification_records(&alice, &bob_id).await;
    let kinds: Vec<_> = records.iter().map(|(record, _)| record.clone()).collect();
    assert_eq!(
        kinds,
        [
            SystemMessage::ContactVerified {
                contact: bob_id.clone()
            },
            SystemMessage::ContactUnverified {
                contact: bob_id.clone()
            },
        ]
    );
    assert_eq!(verification_records(&alice2, &bob_id).await, records);
    bob.drain().await;
    assert!(verification_records(&bob, &alice_id).await.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleting_a_record_deletes_it_on_the_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;
    let bob_id = bob.user_id().clone();
    alice.set_contact_verified(bob_id.clone(), true).await?;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_eq!(verification_records(&alice2, &bob_id).await.len(), 1);

    let chat_id = alice.contact(&bob_id).await.expect("no contact").chat_id;
    let record = alice
        .system_message_id(chat_id, |message| {
            matches!(message, SystemMessage::ContactVerified { .. })
        })
        .await;
    alice.delete_message(record).await?;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    assert!(verification_records(&alice2, &bob_id).await.is_empty());
    assert_eq!(
        alice2.contact_verification(&bob_id).await?,
        ContactVerificationStatus::Verified,
        "deleting the record keeps the mark"
    );

    Ok(())
}
