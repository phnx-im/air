// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use airbackend::auth_service::user_record::UserRecord;
use aircommon::{
    credentials::{
        AsCredential, AsIntermediateCredentialCsr, ClientCredentialCsr, ClientCredentialPayload,
        keys::ClientSigningKey,
    },
    crypto::{
        indexed_aead::{ciphertexts::IndexEncryptable, keys::UserProfileKey},
        signatures::signable::Signable,
    },
    identifiers::{Fqdn, UserId},
};
use openmls::prelude::SignatureScheme;
use sqlx::SqlitePool;

use crate::{
    DisplayName, UserProfile,
    key_stores::indexed_keys::StorableIndexedKey,
    store::StoreNotifier,
    user_profiles::{IndexedUserProfile, update::UserProfileUpdate},
};

use super::{Asset, generate::NewUserProfile};

#[test]
fn backend_interaction() {
    // The user initially creates a user profile
    let user_id = UserId::random("localhost".parse().unwrap());
    let user_profile_key = UserProfileKey::random(&user_id).unwrap();
    let display_name = DisplayName::from_str("Alice").unwrap();
    let profile_picture = Some(Asset::Value(vec![1, 2, 3]));
    let (credential_csr, signing_key) =
        ClientCredentialCsr::new(user_id.clone(), SignatureScheme::ED25519).unwrap();

    let encrypted_user_profile = NewUserProfile::new(
        &signing_key,
        user_id.clone(),
        user_profile_key.index().clone(),
        display_name.clone(),
        profile_picture.clone(),
    )
    .unwrap()
    .skip_storage()
    .encrypt_with_index(&user_profile_key)
    .unwrap();

    // The server then stores it as part of user creation
    let mut user_record = UserRecord::new(user_id.clone(), encrypted_user_profile.clone());

    // Other clients can now load the user profile based on the index of the user profile key
    assert!(
        user_record
            .clone()
            .into_user_profile(user_profile_key.index())
            .is_some()
    );

    // To sign the update we need a full client credential
    let domain = Fqdn::from_str("localhost").unwrap();
    let (_as_credential, ac_sk) =
        AsCredential::new(SignatureScheme::ED25519, domain.clone(), None).unwrap();
    let (as_intermediate_credential_csr, aic_sk) =
        AsIntermediateCredentialCsr::new(SignatureScheme::ED25519, domain.clone()).unwrap();
    let as_intermediate_credential = as_intermediate_credential_csr.sign(&ac_sk, None).unwrap();
    let aic_sk = aic_sk.convert();
    let client_credential = ClientCredentialPayload::new(
        credential_csr,
        None,
        *as_intermediate_credential.fingerprint(),
    )
    .sign(&aic_sk)
    .unwrap();
    let client_sk = ClientSigningKey::from_prelim_key(signing_key, client_credential).unwrap();

    // Now the user wants to update their profile
    // (To simulate loading it from the DB, we just create a new one here)
    let current_profile = IndexedUserProfile {
        user_id: user_id.clone(),
        epoch: 0,
        decryption_key_index: user_profile_key.index().clone(),
        display_name,
        profile_picture: profile_picture.clone(),
    };
    let new_user_profile = UserProfile {
        user_id: user_id.clone(),
        display_name: "Alice Wonderland".parse().unwrap(),
        profile_picture: None,
    };
    let new_user_profile_key = UserProfileKey::random(&user_id).unwrap();
    let new_encrypted_user_profile = UserProfileUpdate::update_own_profile(
        current_profile,
        new_user_profile,
        new_user_profile_key.index().clone(),
        &client_sk,
    )
    .unwrap()
    .skip_storage()
    .encrypt_with_index(&new_user_profile_key)
    .unwrap();

    // Now the server can store/stage the user profile
    user_record.stage_user_profile(new_encrypted_user_profile.clone());

    // If another client tries to load the user profile using the old key it
    // should still work and return the old profile
    let returned_user_profile = user_record
        .clone()
        .into_user_profile(user_profile_key.index())
        .unwrap();
    assert_eq!(returned_user_profile, encrypted_user_profile);

    // Now the user has finished the update and tells the server to merge it
    user_record.merge_user_profile().unwrap();

    // If we try to load the user profile using the old key it should fail
    assert!(
        user_record
            .clone()
            .into_user_profile(user_profile_key.index())
            .is_none()
    );

    // If we try to load the user profile using the new key it should work
    let returned_user_profile = user_record
        .clone()
        .into_user_profile(new_user_profile_key.index())
        .unwrap();
    assert_eq!(returned_user_profile, new_encrypted_user_profile);
}

#[sqlx::test]
fn profile_deletion_trigger(pool: SqlitePool) {
    // Create a user profile
    let user_id = UserId::random("localhost".parse().unwrap());
    let display_name = DisplayName::from_str("Alice").unwrap();
    let profile_picture = Some(Asset::Value(vec![1, 2, 3]));
    let (_credential_csr, signing_key) =
        ClientCredentialCsr::new(user_id.clone(), SignatureScheme::ED25519).unwrap();

    let user_profile_key = UserProfileKey::random(&user_id).unwrap();
    user_profile_key.store(&pool).await.unwrap();

    let _user_profile = NewUserProfile::new(
        &signing_key,
        user_id.clone(),
        user_profile_key.index().clone(),
        display_name.clone(),
        profile_picture.clone(),
    )
    .unwrap()
    .store(&pool, &mut StoreNotifier::noop())
    .await
    .unwrap();

    // The user profile key should be removed when we delete the user profile
    // (We delete directly here. In practice, profiles are deleted via a trigger
    // when the user is removed from the last shared group. This is tested in the
    // integration test called `user_deletion_triggers`.)
    delete_user_profile(&pool).await.unwrap();
    let loaded_key = UserProfileKey::load(&pool, user_profile_key.index()).await;
    assert!(matches!(loaded_key, Err(sqlx::Error::RowNotFound)));
}

async fn delete_user_profile(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM user").execute(executor).await?;
    Ok(())
}
