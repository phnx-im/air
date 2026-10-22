// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Registering users, client persistence, and the registration gates
//! (invitation codes, adaptive registration, push admission).

use std::{assert_matches, fs};

use airapiclient::ApiClient;
use airbackend::settings::{
    AdmissionSettings, RegistrationPolicy, RegistrationSettings, RegistrationThreshold,
};
use aircommon::{
    identifiers::UserId,
    messages::push_token::{PushToken, PushTokenOperator},
    registration::{AdmissionSession, ChallengeKind, RegistrationChallenge},
};
use aircoreclient::clients::{CoreUser, registration::RegistrationError, store::ClientRecord};
use airserver_test_harness::utils::setup::{TestBackend, TestBackendParams, TestUser};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_user() {
    let mut setup = TestBackend::single().await;
    setup.add_user().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn client_db_persists_until_user_deleted() {
    // Create and persist the user.
    let mut setup = TestBackend::single().await;
    let alice = setup.add_persisted_user().await;

    let db_path = setup.temp_dir().to_owned();

    // The client DB is named by the random client record ID.
    let client_record_id = ClientRecord::load_all_from_air_db(db_path.to_str().unwrap())
        .await
        .unwrap()
        .into_iter()
        .find(|record| record.user_id == *alice.user_id())
        .unwrap()
        .client_record_id;
    let client_db_path = db_path.join(format!("{client_record_id}.db"));
    assert!(client_db_path.exists());

    // Try to load the user from the database.
    CoreUser::load_with_server_url(
        db_path.to_str().unwrap(),
        client_record_id,
        Some(setup.server_url()),
    )
    .await
    .unwrap();

    setup.delete_user(&alice).await;

    assert!(!client_db_path.exists());

    // Without a client record, the user cannot be loaded.
    assert!(
        CoreUser::load_with_server_url(
            db_path.to_str().unwrap(),
            client_record_id,
            Some(setup.server_url()),
        )
        .await
        .is_err()
    );

    fs::remove_file(db_path.join("air.db")).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invitation_code_is_single_use_unless_unredeemable() {
    const UNREDEEMABLE_CODE: &str = "E111E000";
    let setup = TestBackend::single_with_params(TestBackendParams {
        registration: RegistrationSettings {
            policy: RegistrationPolicy::Required,
            ..Default::default()
        },
        unredeemable_code: Some(UNREDEEMABLE_CODE.to_owned()),
        ..Default::default()
    })
    .await;

    // working code
    let user_id = UserId::random(setup.domain().clone());
    let code = setup.invitation_codes().first().unwrap();
    assert!(
        TestUser::try_new(&user_id, setup.server_url().clone(), Some(code))
            .await
            .is_ok()
    );

    // code used twice
    let user_id = UserId::random(setup.domain().clone());
    let code = setup.invitation_codes().first().unwrap();
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), Some(code))
        .await
        .unwrap_err();
    let error = error.downcast::<RegistrationError>().unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);

    // not working code
    let user_id = UserId::random(setup.domain().clone());
    let code = "DUMMY007";
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), Some(code))
        .await
        .unwrap_err();
    let error = error.downcast::<RegistrationError>().unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);

    // unredeemable code (first use)
    let user_id = UserId::random(setup.domain().clone());
    assert!(
        TestUser::try_new(
            &user_id,
            setup.server_url().clone(),
            Some(UNREDEEMABLE_CODE)
        )
        .await
        .is_ok()
    );

    // unredeemable code (second use)
    let user_id = UserId::random(setup.domain().clone());
    assert!(
        TestUser::try_new(
            &user_id,
            setup.server_url().clone(),
            Some(UNREDEEMABLE_CODE)
        )
        .await
        .is_ok()
    );
}

/// An adaptive deployment: bare registration until a threshold is reached, then
/// a challenge, and a challenge-verified registration that leaves the counters
/// where they were.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn adaptive_gate_closes_at_ip_limit() {
    let setup = TestBackend::single_with_params(TestBackendParams {
        registration: adaptive_registration(2, 100),
        ..Default::default()
    })
    .await;

    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();

    // Nothing has registered, so nothing is asked for.
    let info = client.as_get_registration_info().await.unwrap();
    assert!(!info.challenge_required);
    assert_eq!(info.accepted_challenges, [ChallengeKind::InvitationCode]);

    for _ in 0..2 {
        let user_id = UserId::random(setup.domain().clone());
        TestUser::try_new(&user_id, setup.server_url().clone(), None)
            .await
            .unwrap();
    }

    // Every test client shares one address bucket, so the per-address threshold
    // is spent and the gate has closed.
    assert!(
        client
            .as_get_registration_info()
            .await
            .unwrap()
            .challenge_required
    );

    let user_id = UserId::random(setup.domain().clone());
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), None)
        .await
        .unwrap_err()
        .downcast::<RegistrationError>()
        .unwrap();
    assert_matches!(
        error,
        RegistrationError::ChallengeRequired(kinds) if kinds == [ChallengeKind::InvitationCode]
    );

    // A code gets in.
    let code = setup.invitation_codes().first().unwrap().clone();
    let user_id = UserId::random(setup.domain().clone());
    TestUser::try_new(&user_id, setup.server_url().clone(), Some(&code))
        .await
        .unwrap();

    // The gated registration was vouched for, so it did not count against the
    // next one either.
    assert!(
        client
            .as_get_registration_info()
            .await
            .unwrap()
            .challenge_required
    );

    // A code the server has already spent does not get in twice.
    let user_id = UserId::random(setup.domain().clone());
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), Some(&code))
        .await
        .unwrap_err()
        .downcast::<RegistrationError>()
        .unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);
}

/// The total threshold closes the gate for an address that has registered
/// nothing itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn adaptive_gate_closes_at_total_limit() {
    let setup = TestBackend::single_with_params(TestBackendParams {
        registration: adaptive_registration(100, 1),
        ..Default::default()
    })
    .await;

    let user_id = UserId::random(setup.domain().clone());
    TestUser::try_new(&user_id, setup.server_url().clone(), None)
        .await
        .unwrap();

    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    assert!(
        client
            .as_get_registration_info()
            .await
            .unwrap()
            .challenge_required
    );
}

/// An open gate leaves a code the request carried anyway unspent, so a gate
/// that opens between discovery and registration does not burn it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn open_gate_does_not_spend_code() {
    // Room to spare on both thresholds, so the gate is open while codes exist.
    let setup = TestBackend::single_with_params(TestBackendParams {
        registration: adaptive_registration(100, 100),
        ..Default::default()
    })
    .await;

    let code = setup.invitation_codes().first().unwrap().clone();

    // Two registrations on one code only work while the code goes unspent.
    for _ in 0..2 {
        let user_id = UserId::random(setup.domain().clone());
        TestUser::try_new(&user_id, setup.server_url().clone(), Some(&code))
            .await
            .unwrap();
    }
}

/// The push-admission challenge end to end, from opening a session to finding
/// the endpoint's quota spent.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn push_admission_requires_session_and_challenge() {
    let setup = TestBackend::single_with_params(TestBackendParams {
        registration: RegistrationSettings {
            policy: RegistrationPolicy::Required,
            challenges: vec![ChallengeKind::AdmissionSession],
            // One account per endpoint, so the last step finds the quota spent.
            admission: AdmissionSettings {
                quotas: vec![RegistrationThreshold {
                    limit: 1,
                    window: chrono::Duration::days(1),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    })
    .await;
    let domain = setup.domain().clone();

    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let info = client.as_get_registration_info().await.unwrap();
    assert!(info.challenge_required);
    assert_eq!(info.accepted_challenges, [ChallengeKind::AdmissionSession]);

    // A bare registration is told what would answer for it.
    let user_id = UserId::random(domain.clone());
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), None)
        .await
        .unwrap_err()
        .downcast::<RegistrationError>()
        .unwrap();
    assert_matches!(
        error,
        RegistrationError::ChallengeRequired(kinds) if kinds == [ChallengeKind::AdmissionSession]
    );

    let endpoint = PushToken::new(PushTokenOperator::Google, "fcm-endpoint".to_owned());
    let session = client.as_create_admission_session(&endpoint).await.unwrap();
    let challenge = setup
        .last_sent_challenge()
        .expect("no challenge reached the endpoint");
    let answered = AdmissionSession {
        session_id: session.session_id,
        challenge: challenge.clone(),
    };

    // The session id alone admits nothing, and the challenge is the half a
    // plain HTTP caller does not have.
    let user_id = UserId::random(domain.clone());
    let error = TestUser::try_new_with_challenge(
        &user_id,
        setup.server_url().clone(),
        Some(RegistrationChallenge::AdmissionSession(AdmissionSession {
            session_id: session.session_id,
            challenge: "not the challenge".to_owned(),
        })),
    )
    .await
    .unwrap_err()
    .downcast::<RegistrationError>()
    .unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);

    // The challenge alone admits nothing either, which keeps the push service
    // from redeeming what it carried.
    let user_id = UserId::random(domain.clone());
    let error = TestUser::try_new_with_challenge(
        &user_id,
        setup.server_url().clone(),
        Some(RegistrationChallenge::AdmissionSession(AdmissionSession {
            session_id: Uuid::new_v4(),
            challenge,
        })),
    )
    .await
    .unwrap_err()
    .downcast::<RegistrationError>()
    .unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);

    let user_id = UserId::random(domain.clone());
    TestUser::try_new_with_challenge(
        &user_id,
        setup.server_url().clone(),
        Some(RegistrationChallenge::AdmissionSession(answered.clone())),
    )
    .await
    .unwrap();

    // The session is spent, so it does not admit a second account.
    let user_id = UserId::random(domain.clone());
    let error = TestUser::try_new_with_challenge(
        &user_id,
        setup.server_url().clone(),
        Some(RegistrationChallenge::AdmissionSession(answered)),
    )
    .await
    .unwrap_err()
    .downcast::<RegistrationError>()
    .unwrap();
    assert_matches!(error, RegistrationError::ChallengeRejected);

    // The endpoint has had the account its quota allows, so the client gets a
    // session no challenge arrives for, which is what a lost push looks like.
    let sent = setup.sent_challenge_count();
    client.as_create_admission_session(&endpoint).await.unwrap();
    assert_eq!(setup.sent_challenge_count(), sent);
}

fn adaptive_registration(per_ip: u64, total: u64) -> RegistrationSettings {
    RegistrationSettings {
        policy: RegistrationPolicy::Adaptive,
        perip: vec![RegistrationThreshold {
            limit: per_ip,
            window: chrono::Duration::days(1),
        }],
        total: vec![RegistrationThreshold {
            limit: total,
            window: chrono::Duration::days(1),
        }],
        ..Default::default()
    }
}
