// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{codec::PersistenceCodec, identifiers::UserId};
use airserver_test_harness::utils::setup::TestBackend;

use crate::{
    clients::store::{ClientRecord, ClientRecordState, UserCreationState},
    utils::persistence::open_db_in_memory,
};

use super::api_clients::ApiClients;

#[tokio::test(flavor = "multi_thread")]
async fn user_stages() -> anyhow::Result<()> {
    // Set up backend
    let setup = TestBackend::single().await;
    let server_url = format!("http://{}", setup.listen_addr());

    let user_id = UserId::random("example.com".parse().unwrap());

    let air_db = open_db_in_memory().await?;
    let client_db = open_db_in_memory().await?;

    let api_clients = ApiClients::new(user_id.domain().clone(), server_url.clone());

    let computed_state =
        UserCreationState::new(&client_db, &air_db, user_id.clone(), server_url, None).await?;

    // There should now be a client record state in the air db.
    let client_records = ClientRecord::load_all(&air_db).await?;
    assert!(client_records.len() == 1);
    let client_record = client_records.first().unwrap();
    assert!(client_record.user_id == user_id);
    assert!(matches!(
        client_record.client_record_state,
        ClientRecordState::InProgress
    ));

    // If we load a user state now, it should be the basic user data state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(loaded_state, UserCreationState::BasicUserData(_)));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We now continue down the path of creating a user.
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the initial user state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(
        loaded_state,
        UserCreationState::InitialUserState(_)
    ));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We take the next step
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the post registration init state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(
        loaded_state,
        UserCreationState::PostRegistrationInitState(_)
    ));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We take the next step
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the unfinalized registration state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(
        loaded_state,
        UserCreationState::UnfinalizedRegistrationState(_)
    ));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We take the next step
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the AS registered user state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(
        loaded_state,
        UserCreationState::AsRegisteredUserState(_)
    ));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We take the next step
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the QS registered user state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(
        loaded_state,
        UserCreationState::QsRegisteredUserState(_)
    ));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    // We take the final step
    let computed_state = loaded_state
        .step(&air_db, &client_db, &api_clients)
        .await
        .unwrap();

    // If we load a user state now, it should be the final user state.
    let loaded_state = UserCreationState::load(&client_db, &user_id)
        .await?
        .unwrap();
    assert!(matches!(loaded_state, UserCreationState::FinalUserState(_)));
    assert_eq!(
        PersistenceCodec::to_vec(&computed_state).unwrap(),
        PersistenceCodec::to_vec(&loaded_state).unwrap()
    );

    Ok(())
}
