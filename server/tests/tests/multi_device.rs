use aircoreclient::clients::CoreUser;
use airserver_test_harness::utils::setup::TestBackend;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test multi-device pairing session", skip_all)]
async fn multi_device_pairing_session() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = Some(setup.server_url());
    let alice = setup.add_user().await;

    let (session_id_tx, session_id_rx) = tokio::sync::oneshot::channel();

    let new_device_task = tokio::spawn(async move {
        let old_device_message =
            CoreUser::provision_multi_device_pairing(domain, server_url, session_id_tx)
                .await
                .unwrap();

        assert_eq!(old_device_message, "pong!");
    });

    let session_id = session_id_rx.await.unwrap();

    // the old device scans/types the session ID
    let new_device_message = setup
        .get_user(&alice)
        .user()
        .link_multi_device_pairing(session_id)
        .await
        .unwrap();

    assert_eq!(new_device_message, "ping!");

    new_device_task.await.unwrap();
}
