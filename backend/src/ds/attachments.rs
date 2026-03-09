// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// TODO: Adjust the names of the APIs <https://github.com/phnx-im/air/issues/1073>

use aircommon::time::ExpirationData;
use airprotos::{
    common::v1::{
        AttachmentTooLargeDetail, StatusDetails, StatusDetailsCode, status_details::Detail,
    },
    delivery_service::v1::{
        GetAttachmentUrlResponse, HeaderEntry, ProvisionAttachmentResponse, SignedPostPolicy,
        StorageObjectType,
    },
};
use aws_sdk_s3::{
    config::http,
    error::{BuildError, SdkError},
    operation::{get_object, put_object},
    presigning::{PresigningConfig, PresigningConfigError},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use displaydoc::Display;
use prost::Message;
use serde::Serialize;
use serde_json::json;
use tonic::{Code, Response, Status};
use tracing::error;
use uuid::Uuid;

use crate::settings::StoragePaths;

use super::{Ds, storage::Storage};

impl Ds {
    pub(super) async fn provision_object(
        &self,
        object_type: StorageObjectType,
        content_length: Option<u64>,
        use_post_policy: bool,
    ) -> Result<ProvisionAttachmentResponse, ProvisionObjectError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(ProvisionObjectError::NoStorageConfigured);
        };

        let object_id = Uuid::new_v4();
        let expiration = ExpirationData::now(storage.settings().upload_expiration);

        let response = if storage.settings().use_post_policy && use_post_policy {
            create_signed_post(storage, object_id, expiration, object_type)
        } else {
            // We still allow content length 0 for legacy clients.
            let content_length = content_length.filter(|content_length| *content_length > 0);
            create_signed_put(storage, object_id, expiration, content_length, object_type).await?
        };
        Ok(response)
    }

    pub(super) async fn get_object_url(
        &self,
        object_id: Uuid,
        object_type: StorageObjectType,
    ) -> Result<Response<GetAttachmentUrlResponse>, GetAttachmentUrlError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(GetAttachmentUrlError::NoStorageConfigured);
        };

        let expiration = ExpirationData::now(storage.settings().download_expiration);
        let not_before: DateTime<Utc> = expiration.not_before().into();
        let not_after: DateTime<Utc> = expiration.not_after().into();
        let duration = not_after - not_before;

        let mut presigning_config = PresigningConfig::builder();
        presigning_config.set_start_time(Some(not_before.into()));
        presigning_config.set_expires_in(Some(duration.to_std()?));
        let presigning_config = presigning_config.build()?;

        let key = storage_key(&storage.settings().storage_paths, object_id, object_type);
        let request = storage
            .client()
            .get_object()
            .bucket(storage.settings().bucket.clone())
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(Box::new)?;

        let url = request.uri().to_owned();
        let headers: Vec<HeaderEntry> = request
            .headers()
            .map(|(k, v)| HeaderEntry {
                key: k.to_owned(),
                value: v.to_owned(),
            })
            .collect();

        Ok(Response::new(GetAttachmentUrlResponse {
            download_url_expiration: Some(expiration.into()),
            download_url: url,
            download_headers: headers,
        }))
    }
}

async fn create_signed_put(
    storage: &Storage,
    object_id: Uuid,
    expiration: ExpirationData,
    content_length: Option<u64>,
    object_type: StorageObjectType,
) -> Result<ProvisionAttachmentResponse, ProvisionObjectError> {
    let not_before: DateTime<Utc> = expiration.not_before().into();
    let not_after: DateTime<Utc> = expiration.not_after().into();
    let duration = not_after - not_before;

    let mut presigning_config = PresigningConfig::builder();
    presigning_config.set_start_time(Some(not_before.into()));
    presigning_config.set_expires_in(Some(duration.to_std()?));
    let presigning_config = presigning_config.build()?;

    let key = storage_key(&storage.settings().storage_paths, object_id, object_type);
    let request = storage.client().put_object().bucket("data").key(key);

    let settings = storage.settings();

    let request = if let Some(content_length) = content_length {
        if settings.max_attachment_size < content_length {
            return Err(ProvisionObjectError::DataTooLarge {
                max_size: settings.max_attachment_size,
                actual_size: content_length,
            });
        }
        request.set_content_length(Some(content_length as i64))
    } else if settings.require_content_length {
        return Err(ProvisionObjectError::ContentLengthRequired);
    } else {
        request
    };

    let request = request
        .presigned(presigning_config)
        .await
        .map_err(Box::new)?;

    let url = request.uri().to_owned();
    let header: Vec<HeaderEntry> = request
        .headers()
        .map(|(k, v)| HeaderEntry {
            key: k.to_owned(),
            value: v.to_owned(),
        })
        .collect();

    Ok(ProvisionAttachmentResponse {
        object_id: Some(object_id.into()),
        upload_url_expiration: Some(expiration.into()),
        upload_url: url,
        upload_headers: header,
        post_policy: None,
    })
}

#[derive(Serialize)]
struct Policy {
    expiration: DateTime<Utc>,
    conditions: [serde_json::Value; 6],
}

fn create_signed_post(
    storage: &Storage,
    object_id: Uuid,
    expiration: ExpirationData,
    object_type: StorageObjectType,
) -> ProvisionAttachmentResponse {
    let not_before: DateTime<Utc> = expiration.not_before().into();
    let not_after: DateTime<Utc> = expiration.not_after().into();

    let settings = storage.settings();

    let x_amz_credential = format!(
        "{access_key}/{date}/{region}/s3/aws4_request",
        access_key = settings.access_key_id,
        date = not_before.format("%Y%m%d"),
        region = settings.region,
    );

    let key = storage_key(&storage.settings().storage_paths, object_id, object_type);
    let policy = Policy {
        expiration: not_after,
        conditions: [
            json!({"bucket": "data"}),
            json!({"key": key}),
            json!(["content-length-range", 0, settings.max_attachment_size]),
            json!({"x-amz-credential": x_amz_credential}),
            json!({"x-amz-algorithm": "AWS4-HMAC-SHA256"}),
            json!({"x-amz-date": not_before.format("%Y%m%dT%H%M%SZ").to_string()}),
        ],
    };

    // Note: sigv4a is not supported by minio, which is used in local deployment.
    let signing_key = aws_sigv4::sign::v4::generate_signing_key(
        settings.secret_access_key.as_ref(),
        not_before.into(),
        &settings.region,
        "s3",
    );
    let policy_json = serde_json::to_string(&policy).expect("policy is always serializable");
    let policy_base64 = BASE64_STANDARD.encode(policy_json);
    let signature = aws_sigv4::sign::v4::calculate_signature(signing_key, policy_base64.as_bytes());

    // Note: We just use a simpler path style URL here.
    let upload_url = format!(
        "{endpoint}/{bucket}",
        endpoint = settings.endpoint,
        bucket = "data",
    );

    let post_policy = SignedPostPolicy {
        base64: policy_base64,
        signature: signature.to_string(),
    };

    ProvisionAttachmentResponse {
        object_id: Some(object_id.into()),
        upload_url_expiration: Some(expiration.into()),
        upload_url,
        post_policy: Some(post_policy),
        ..Default::default()
    }
}

fn storage_key(paths: &StoragePaths, object_id: Uuid, object_type: StorageObjectType) -> String {
    let key = object_id.as_simple();
    match object_type {
        // Note: Unspecified is treated as a generic attachment to preserve backwards compatibility
        // with older clients.
        StorageObjectType::Unspecified | StorageObjectType::Attachment => {
            let path = paths.attachments_path.trim_end_matches('/');
            format!("{path}/{key}")
        }
        StorageObjectType::GroupProfile => {
            let path = paths.group_profiles_path.trim_end_matches('/');
            format!("{path}/{key}")
        }
        StorageObjectType::UserProfile => {
            let path = paths.user_profiles_path.trim_end_matches('/');
            format!("{path}/{key}")
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub(super) enum ProvisionObjectError {
    /// Attachments are not supported
    NoStorageConfigured,
    /// Internal error
    Build(#[from] BuildError),
    /// Internal error
    Duration(#[from] chrono::OutOfRangeError),
    /// Internal error
    Presigning(#[from] PresigningConfigError),
    /// Internal error
    Sdk(#[from] Box<SdkError<put_object::PutObjectError, http::HttpResponse>>),
    /// Content length is required
    ContentLengthRequired,
    /// Attachment is too large: {actual_size} bytes > {max_size} bytes
    DataTooLarge { max_size: u64, actual_size: u64 },
}

impl From<ProvisionObjectError> for Status {
    fn from(error: ProvisionObjectError) -> Self {
        let msg = error.to_string();
        match error {
            ProvisionObjectError::NoStorageConfigured => {
                error!("Storage is not configured");
                Status::internal(msg)
            }
            ProvisionObjectError::Build(error) => {
                error!(%error, "Failed to build S3 config");
                Status::internal(msg)
            }
            ProvisionObjectError::Duration(error) => {
                error!(%error, "Failed to convert chrono to std duration");
                Status::internal(msg)
            }
            ProvisionObjectError::Presigning(error) => {
                error!(%error, "Failed to create presigning config");
                Status::internal(msg)
            }
            ProvisionObjectError::Sdk(error) => {
                error!(%error, "Failed to build S3 request");
                Status::internal(msg)
            }
            ProvisionObjectError::ContentLengthRequired => {
                Status::invalid_argument("content length is required")
            }
            ProvisionObjectError::DataTooLarge {
                max_size,
                actual_size,
            } => {
                let message = format!(
                    "data is too large; maximum size is {max_size} bytes, \
                        actual size is {actual_size} bytes",
                );
                Status::with_details(
                    Code::InvalidArgument,
                    message,
                    StatusDetails {
                        code: StatusDetailsCode::VersionUnsupported.into(),
                        detail: Some(Detail::AttachmentTooLarge(AttachmentTooLargeDetail {
                            max_size_bytes: max_size,
                            actual_size_bytes: actual_size,
                        })),
                    }
                    .encode_to_vec()
                    .into(),
                )
            }
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub(super) enum GetAttachmentUrlError {
    /// Attachments are not supported
    NoStorageConfigured,
    /// Internal error
    Build(#[from] BuildError),
    /// Internal error
    Duration(#[from] chrono::OutOfRangeError),
    /// Internal error
    Presigning(#[from] PresigningConfigError),
    /// Internal error
    Sdk(#[from] Box<SdkError<get_object::GetObjectError, http::HttpResponse>>),
}

impl From<GetAttachmentUrlError> for Status {
    fn from(error: GetAttachmentUrlError) -> Self {
        let msg = error.to_string();
        match error {
            GetAttachmentUrlError::NoStorageConfigured => {
                error!("Storage is not configured");
                Status::internal(msg)
            }
            GetAttachmentUrlError::Build(error) => {
                error!(%error, "Failed to build S3 config");
                Status::internal(msg)
            }
            GetAttachmentUrlError::Duration(error) => {
                error!(%error, "Failed to convert chrono to std duration");
                Status::internal(msg)
            }
            GetAttachmentUrlError::Presigning(error) => {
                error!(%error, "Failed to create presigning config");
                Status::internal(msg)
            }
            GetAttachmentUrlError::Sdk(error) => {
                error!(%error, "Failed to build S3 request");
                Status::internal(msg)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::Duration;
    use uuid::uuid;

    use crate::settings::StorageSettings;

    use super::*;

    fn storage() -> Storage {
        let settings = StorageSettings {
            endpoint: "https://region.example.com".to_owned(),
            region: "example-region".to_owned(),
            access_key_id: "EXAMPLEKEY".to_owned(),
            secret_access_key: "EXMPLESECRET".to_owned().into(),
            bucket: "data".to_owned(),
            force_path_style: false,
            upload_expiration: Duration::seconds(60),
            download_expiration: Duration::seconds(60),
            max_attachment_size: 20 * 1024 * 1024,
            use_post_policy: false,
            require_content_length: true,
            storage_paths: Default::default(),
        };
        Storage::new(settings)
    }

    #[tokio::test]
    async fn test_create_signed_put() {
        let object_id = uuid!("ba521fc6-1ec2-4f8e-a85e-3dacc1e96989");
        let at = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let expiration = ExpirationData::from_parts(at.into(), (at + Duration::seconds(60)).into());

        let storage = storage();

        let response = create_signed_put(
            &storage,
            object_id,
            expiration.clone(),
            None,
            StorageObjectType::Unspecified,
        )
        .await;
        assert!(response.is_err());

        let response = create_signed_put(
            &storage,
            object_id,
            expiration,
            Some(42),
            StorageObjectType::Unspecified,
        )
        .await;
        insta::assert_debug_snapshot!(response);
    }

    #[test]
    fn test_create_signed_policy() {
        let object_id = uuid!("ba521fc6-1ec2-4f8e-a85e-3dacc1e96989");
        let at = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let expiration = ExpirationData::from_parts(at.into(), (at + Duration::seconds(60)).into());

        let storage = storage();
        let response = create_signed_post(
            &storage,
            object_id,
            expiration,
            StorageObjectType::Unspecified,
        );

        insta::assert_debug_snapshot!(response);

        let policy = response.post_policy.unwrap();
        let policy_json = BASE64_STANDARD.decode(&policy.base64).unwrap();
        let policy: serde_json::Value = serde_json::from_slice(&policy_json).unwrap();

        insta::assert_debug_snapshot!(policy);
    }

    #[test]
    fn test_storage_key_with_default_paths() {
        let paths = StoragePaths::default();

        let object_id = uuid!("ba521fc6-1ec2-4f8e-a85e-3dacc1e96989");

        // Backwards compatibility with older clients
        let object_type = StorageObjectType::Unspecified;
        let key = storage_key(&paths, object_id, object_type);
        assert_eq!(key, "attachments/ba521fc61ec24f8ea85e3dacc1e96989");

        let object_type = StorageObjectType::Attachment;
        let key = storage_key(&paths, object_id, object_type);
        assert_eq!(key, "attachments/ba521fc61ec24f8ea85e3dacc1e96989");

        let object_type = StorageObjectType::GroupProfile;
        let key = storage_key(&paths, object_id, object_type);
        assert_eq!(key, "group-profiles/ba521fc61ec24f8ea85e3dacc1e96989");

        let object_type = StorageObjectType::UserProfile;
        let key = storage_key(&paths, object_id, object_type);
        assert_eq!(key, "user-profiles/ba521fc61ec24f8ea85e3dacc1e96989");
    }
}
