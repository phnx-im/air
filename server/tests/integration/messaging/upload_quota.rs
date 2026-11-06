// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The per-user quota on provisioned upload bytes.

use std::assert_matches;

use airapiclient::ds_api::DsRequestError;
use aircoreclient::clients::StorageObjectType;
use airprotos::common::v1::{StatusDetails, UploadQuotaExceededDetail, status_details::Detail};
use airserver_test_harness::utils::setup::{TestBackend, TestBackendParams, UploadQuotaSettings};
use chrono::Duration;

const LIMIT_BYTES: u64 = 100;

async fn backend_with_quota() -> TestBackend {
    TestBackend::single_with_params(TestBackendParams {
        upload_quota: Some(UploadQuotaSettings {
            limit_bytes: LIMIT_BYTES,
            window: Duration::hours(1),
        }),
        ..Default::default()
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn uploads_beyond_the_quota_are_rejected() {
    let mut setup = backend_with_quota().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice
        .upload_with_declared_size(chat_id, 60, vec![0; 60])
        .await
        .unwrap();

    let error = alice
        .upload_with_declared_size(chat_id, 60, vec![0; 60])
        .await
        .unwrap_err();
    let error = error.downcast::<DsRequestError>().unwrap();
    let DsRequestError::Tonic(status) = error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(status.code(), tonic::Code::ResourceExhausted);
    let detail = StatusDetails::from_status(&status).and_then(|details| details.detail);
    assert_matches!(
        detail,
        Some(Detail::UploadQuotaExceeded(UploadQuotaExceededDetail {
            retry_after_secs: 1..=3600,
            limit_bytes: LIMIT_BYTES,
        }))
    );

    // The quota is per user.
    bob.upload_with_declared_size(chat_id, 60, vec![0; 60])
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn debug_logs_do_not_count_against_the_upload_quota() {
    let mut setup = backend_with_quota().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice
        .upload_with_declared_size(chat_id, LIMIT_BYTES as i64, vec![0; LIMIT_BYTES as usize])
        .await
        .unwrap();

    alice
        .upload_user_attachment(StorageObjectType::DebugLogs, vec![0; 60])
        .await
        .unwrap()
        .unwrap();
}
