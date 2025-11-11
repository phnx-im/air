// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::AttachmentId, time::ExpirationData};
use airprotos::delivery_service::v1::{
    GetAttachmentUrlResponse, HeaderEntry, ProvisionAttachmentPayload, ProvisionAttachmentResponse,
    SignedPostPolicy,
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
use serde::Serialize;
use serde_json::json;
use tonic::{Response, Status};
use tracing::error;
use uuid::Uuid;

use super::{Ds, storage::Storage};

impl Ds {
    pub(super) async fn provision_attachment(
        &self,
        payload: ProvisionAttachmentPayload,
    ) -> Result<Response<ProvisionAttachmentResponse>, ProvisionAttachmentError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(ProvisionAttachmentError::NoStorageConfigured);
        };

        let attachment_id = Uuid::new_v4();

        let expiration = ExpirationData::now(storage.attributes().upload_expiration);

        let response = if !payload.use_post_policy {
            create_signed_put(storage, attachment_id, expiration).await?
        } else {
            create_signed_post(storage, attachment_id, expiration)
        };
        Ok(Response::new(response))
    }

    pub(super) async fn get_attachment_url(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<Response<GetAttachmentUrlResponse>, GetAttachmentUrlError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(GetAttachmentUrlError::NoStorageConfigured);
        };

        let expiration = ExpirationData::now(storage.attributes().download_expiration);
        let not_before: DateTime<Utc> = expiration.not_before().into();
        let not_after: DateTime<Utc> = expiration.not_after().into();
        let duration = not_after - not_before;

        let mut presigning_config = PresigningConfig::builder();
        presigning_config.set_start_time(Some(not_before.into()));
        presigning_config.set_expires_in(Some(duration.to_std()?));
        let presigning_config = presigning_config.build()?;

        let request = storage
            .client()
            .get_object()
            .bucket("data")
            .key(attachment_id.uuid().as_simple().to_string())
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
    attachment_id: Uuid,
    expiration: ExpirationData,
) -> Result<ProvisionAttachmentResponse, ProvisionAttachmentError> {
    let not_before: DateTime<Utc> = expiration.not_before().into();
    let not_after: DateTime<Utc> = expiration.not_after().into();
    let duration = not_after - not_before;

    let mut presigning_config = PresigningConfig::builder();
    presigning_config.set_start_time(Some(not_before.into()));
    presigning_config.set_expires_in(Some(duration.to_std()?));
    let presigning_config = presigning_config.build()?;

    let request = storage
        .client()
        .put_object()
        .bucket("data")
        .key(attachment_id.as_simple().to_string())
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
        attachment_id: Some(attachment_id.into()),
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
    attachment_id: Uuid,
    expiration: ExpirationData,
) -> ProvisionAttachmentResponse {
    let not_before: DateTime<Utc> = expiration.not_before().into();
    let not_after: DateTime<Utc> = expiration.not_after().into();

    let attributes = storage.attributes();

    let x_amz_credential = format!(
        "{access_key}/{date}/{region}/s3/aws4_request",
        access_key = attributes.access_key_id,
        date = not_before.format("%Y%m%d"),
        region = attributes.region,
    );

    let policy = Policy {
        expiration: not_after,
        conditions: [
            json!({"bucket": "data"}),
            json!({"key": attachment_id.as_simple().to_string()}),
            json!(["content-length-range", 0, attributes.max_attachment_size]),
            json!({"x-amz-credential": x_amz_credential}),
            json!({"x-amz-algorithm": "AWS4-HMAC-SHA256"}),
            json!({"x-amz-date": not_before.format("%Y%m%dT%H%M%SZ").to_string()}),
        ],
    };

    // Note: sigv4a is not supported by minio, which is used in local deployment.
    let signing_key = aws_sigv4::sign::v4::generate_signing_key(
        attributes.secret_access_key.as_ref(),
        not_before.into(),
        &attributes.region,
        "s3",
    );
    let policy_json = serde_json::to_string(&policy).expect("policy is always serializable");
    let policy_base64 = BASE64_STANDARD.encode(policy_json);
    let signature = aws_sigv4::sign::v4::calculate_signature(signing_key, policy_base64.as_bytes());

    // Note: We just use a simpler path style URL here.
    let upload_url = format!(
        "{endpoint}/{bucket}",
        endpoint = attributes.endpoint_url,
        bucket = "data",
    );

    let post_policy = SignedPostPolicy {
        base64: policy_base64,
        signature: signature.to_string(),
    };

    ProvisionAttachmentResponse {
        attachment_id: Some(attachment_id.into()),
        upload_url_expiration: Some(expiration.into()),
        upload_url,
        post_policy: Some(post_policy),
        ..Default::default()
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub(super) enum ProvisionAttachmentError {
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
}

impl From<ProvisionAttachmentError> for Status {
    fn from(error: ProvisionAttachmentError) -> Self {
        let msg = error.to_string();
        match error {
            ProvisionAttachmentError::NoStorageConfigured => {
                error!("Storage is not configured");
                Status::internal(msg)
            }
            ProvisionAttachmentError::Build(error) => {
                error!(%error, "Failed to build S3 config");
                Status::internal(msg)
            }
            ProvisionAttachmentError::Duration(error) => {
                error!(%error, "Failed to convert chrono to std duration");
                Status::internal(msg)
            }
            ProvisionAttachmentError::Presigning(error) => {
                error!(%error, "Failed to create presigning config");
                Status::internal(msg)
            }
            ProvisionAttachmentError::Sdk(error) => {
                error!(%error, "Failed to build S3 request");
                Status::internal(msg)
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
            endpoint: "https://s3.us-east-1.amazonaws.com".to_owned(),
            region: "eu-west-1".to_owned(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_owned(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_owned().into(),
            force_path_style: false,
            upload_expiration: Duration::seconds(60),
            download_expiration: Duration::seconds(60),
            max_attachment_size: 20 * 1024 * 1024,
        };
        Storage::new(settings)
    }

    #[tokio::test]
    async fn test_create_signed_put() {
        let attachment_id = uuid!("ba521fc6-1ec2-4f8e-a85e-3dacc1e96989");
        let at = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let expiration = ExpirationData::from_parts(at.into(), (at + Duration::seconds(60)).into());

        let storage = storage();
        let response = create_signed_put(&storage, attachment_id, expiration).await;

        insta::assert_debug_snapshot!(response);
    }

    #[test]
    fn test_create_signed_policy() {
        let attachment_id = uuid!("ba521fc6-1ec2-4f8e-a85e-3dacc1e96989");
        let at = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let expiration = ExpirationData::from_parts(at.into(), (at + Duration::seconds(60)).into());

        let storage = storage();
        let response = create_signed_post(&storage, attachment_id, expiration);

        insta::assert_debug_snapshot!(response);

        let policy = response.post_policy.unwrap();
        let policy_json = BASE64_STANDARD.decode(&policy.base64).unwrap();
        let policy: serde_json::Value = serde_json::from_slice(&policy_json).unwrap();

        insta::assert_debug_snapshot!(policy);
    }
}
