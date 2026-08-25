// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fmt, fs,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use airbackend::{
    auth_service::admission::{ChallengeSendError, ChallengeSender},
    qs::{PushNotificationError, PushNotificationProvider},
    settings::{ApnsSettings, FcmSettings},
};
use aircommon::messages::push_token::{PushToken, PushTokenOperator};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{
    Client, StatusCode,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

const DEFAULT_APNS_ENDPOINT: &str = "https://api.push.apple.com";

const DEFAULT_APNS_TOPIC: &str = "ms.air";

#[derive(Debug, Serialize)]
struct FcmClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

// Struct for the Google OAuth2 response
#[derive(Debug, Deserialize)]
struct OauthSuccessResponse {
    access_token: String,
    expires_in: u64,
    #[expect(dead_code)]
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct OauthErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApnsClaims<'a> {
    iss: &'a str,
    iat: u64,
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
#[cfg_attr(test, derive(PartialEq, Eq))]
struct ApnsToken {
    jwt: String,
    issued_at: u64, // Seconds since UNIX_EPOCH
}

impl fmt::Debug for ApnsToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApnsToken")
            .field("jwt", &"[[REDACTED]]")
            .field("issued_at", &self.issued_at)
            .finish()
    }
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
struct FcmToken {
    token: String,
    expires_at: u64, // Seconds since UNIX_EPOCH
}

impl fmt::Debug for FcmToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FcmToken")
            .field("token", &"[[REDACTED]]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl FcmToken {
    fn token(&self) -> &str {
        &self.token
    }

    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at
    }
}

#[derive(Clone)]
struct FcmState {
    service_account: Arc<ServiceAccount>,
    // Note: zeroized in <https://github.com/Keats/jsonwebtoken/issues/337>
    private_key: EncodingKey,
    token: Arc<Mutex<Option<FcmToken>>>,
}

impl fmt::Debug for FcmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FcmState")
            .field("service_account", &self.service_account)
            .field("private_key", &"[[REDACTED]]")
            .field("token", &self.token)
            .finish()
    }
}

#[derive(Clone)]
struct ApnsState {
    key_id: String,
    team_id: String,
    topic: String,
    endpoint: String,
    // Note: zeroized in <https://github.com/Keats/jsonwebtoken/issues/337>
    private_key: EncodingKey,
    token: Arc<Mutex<Option<ApnsToken>>>,
}

impl fmt::Debug for ApnsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApnsState")
            .field("key_id", &self.key_id)
            .field("team_id", &self.team_id)
            .field("topic", &self.topic)
            .field("endpoint", &self.endpoint)
            .field("private_key", &"[[REDACTED]]")
            .field("token", &self.token)
            .finish()
    }
}

/// What went wrong before APNs answered. The request URL carries the device
/// token, so it is stripped from the error before the message is built.
#[derive(Debug, thiserror::Error)]
enum ApnsPostError {
    #[error("failed to create the APNs JWT: {0}")]
    Jwt(String),
    #[error("the APNs request failed: {0}")]
    Network(String),
}

/// What went wrong before FCM answered.
#[derive(Debug, thiserror::Error)]
enum FcmPostError {
    #[error("the service account names no project")]
    MissingProjectId,
    #[error("failed to issue the FCM bearer token: {0}")]
    OAuth(String),
    #[error("the FCM request failed: {0}")]
    Network(String),
}

#[derive(Debug, Deserialize, Zeroize, ZeroizeOnDrop)]
struct ServiceAccount {
    #[serde(rename = "type")]
    key_type: Option<String>,
    project_id: Option<String>,
    private_key_id: Option<String>,
    private_key: String,
    client_email: String,
    client_id: Option<String>,
    auth_uri: Option<String>,
    token_uri: String,
    auth_provider_x509_cert_url: Option<String>,
    client_x509_cert_url: Option<String>,
    universe_domain: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProductionPushNotificationProvider {
    client: reqwest::Client,
    fcm_state: Option<FcmState>,
    apns_state: Option<ApnsState>,
}

impl ProductionPushNotificationProvider {
    // Create a new ProductionPushNotificationProvider. If the settings are
    // None, the provider will effectively not send push notifications for that
    // platform.
    pub fn new(
        fcm_settings: Option<FcmSettings>,
        apns_settings: Option<ApnsSettings>,
    ) -> anyhow::Result<Self> {
        // Read the FCN service account file
        let fcm_state = if let Some(fcm_settings) = fcm_settings {
            let service_account = fs::read_to_string(fcm_settings.path)?;
            let service_account: ServiceAccount = serde_json::from_str(&service_account)?;
            let private_key = EncodingKey::from_rsa_pem(service_account.private_key.as_bytes())?;

            Some(FcmState {
                service_account: Arc::new(service_account),
                private_key,
                token: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        };

        // Read the parameters for APNS
        let apns_state = if let Some(apns_settings) = apns_settings {
            let private_key_p8 = fs::read_to_string(&apns_settings.privatekeypath)?;
            let private_key = EncodingKey::from_ec_pem(private_key_p8.as_bytes())?;

            Some(ApnsState {
                key_id: apns_settings.keyid,
                team_id: apns_settings.teamid,
                topic: apns_settings
                    .topic
                    .unwrap_or_else(|| DEFAULT_APNS_TOPIC.to_owned()),
                endpoint: apns_settings
                    .endpoint
                    .unwrap_or_else(|| DEFAULT_APNS_ENDPOINT.to_owned()),
                private_key,
                token: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        };

        Ok(Self {
            client: Client::new(),
            fcm_state,
            apns_state,
        })
    }

    async fn issue_fcm_token(
        &self,
        fcm_auth_url: &str,
    ) -> Result<FcmToken, Box<dyn std::error::Error + Send + Sync>> {
        // TODO #237: Proactively refresh the token before it expires
        let fcm_state = self.fcm_state.as_ref().ok_or("Missing Service Account")?;

        // Check whether we already have a token and if it is still valid
        let mut token_option = fcm_state.token.lock().await;
        if let Some(token) = token_option.as_ref()
            && !token.is_expired()
        {
            return Ok(token.clone());
        }

        // Create the JWT
        let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let jwt = create_google_jwt_token(
            &fcm_state.private_key,
            &fcm_state.service_account.client_email,
            iat,
        )?;

        // Send the JWT to Google's OAuth2 token endpoint and get a bearer token
        // back
        let response = self
            .client
            .post(fcm_auth_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await?;

        // Check if the request was successful
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let response = serde_json::from_str::<OauthErrorResponse>(&body)?;
            return Err(format!(
                "Error response from Google OAuth2: {} {}",
                response.error,
                response.error_description.unwrap_or_default()
            )
            .into());
        }

        let token_response: OauthSuccessResponse = serde_json::from_str(&body)?;

        // Create the FcmToken
        let fcm_token = FcmToken {
            token: token_response.access_token,
            // Save the expiration time
            expires_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
                + token_response.expires_in,
        };

        // Store the token
        *token_option = Some(fcm_token.clone());

        Ok(fcm_token)
    }

    /// Return a JWT for APNS. If the token is older than 40 minutes, a new
    /// token is issued (as JWTs must be between 20 and 60 minutes old).
    async fn issue_apns_jwt(&self) -> Result<ApnsToken, Box<dyn std::error::Error>> {
        // TODO #237: Proactively refresh the jwt before it expires
        let apns_state = self.apns_state.as_ref().ok_or("Missing ApnsState")?;

        // Check whether we already have a token and if it is still valid, i.e.
        // not older than 40 minutes
        let mut token_option = apns_state.token.lock().await;

        if let Some(token) = &*token_option {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if now < token.issued_at + 60 * 40 {
                return Ok(token.clone());
            }
        }

        // Create the JWT claims
        let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let claims = ApnsClaims {
            iss: &apns_state.team_id,
            iat,
        };

        // Create the JWT header
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(apns_state.key_id.clone());

        // Encode the JWT
        let jwt = encode(&header, &claims, &apns_state.private_key)?;
        let token = ApnsToken {
            jwt,
            issued_at: iat,
        };

        // Store the JWT and update the last issuance time
        *token_option = Some(token.clone());

        Ok(token)
    }

    async fn push_google(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        // If we don't have an FCM state, we can't send push notifications
        let Some(fcm_state) = &self.fcm_state else {
            return Ok(());
        };

        let (status, response) = self
            .post_fcm(fcm_state, push_token.token(), json!({ "data": "" }))
            .await
            .map_err(|error| match error {
                FcmPostError::MissingProjectId => PushNotificationError::InvalidConfiguration(
                    "missing project id in the service account".to_string(),
                ),
                FcmPostError::OAuth(error) => PushNotificationError::OAuthError(error),
                FcmPostError::Network(error) => PushNotificationError::NetworkError(error),
            })?;

        match status {
            StatusCode::OK => Ok(()),
            // If the token is invalid, we might want to know it and
            // delete it
            StatusCode::NOT_FOUND => Err(PushNotificationError::InvalidToken(response)),
            // If the status code is not OK or NOT_FOUND, we might want to
            // log the error
            s => Err(PushNotificationError::Other(format!(
                "Unexpected status code: {s} with body: {response}"
            ))),
        }
    }

    async fn push_apple(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        // If we don't have an APNS state, we can't send push notifications
        let Some(apns_state) = &self.apns_state else {
            return Ok(());
        };

        let body = json!({
            "aps": {
                "alert": {
                    "title": "Empty notification",
                    "body": "This artefact should disappear once the app is in public beta."
                },
                "mutable-content": 1
            },
            "data": "data"
        });

        let (status, response) = self
            .post_apns(apns_state, push_token.token(), "alert", "10", 0, body)
            .await
            .map_err(|error| match error {
                ApnsPostError::Jwt(error) => PushNotificationError::JwtCreationError(error),
                ApnsPostError::Network(error) => PushNotificationError::NetworkError(error),
            })?;

        match status {
            StatusCode::OK => Ok(()),
            // If the token is invalid, we might want to know it and
            // delete it
            StatusCode::GONE => Err(PushNotificationError::InvalidToken(response)),
            // If the status code is not OK or GONE, we might want to
            // log the error
            s => Err(PushNotificationError::Other(format!(
                "Unexpected status code: {s} with body: {response}"
            ))),
        }
    }

    /// Sends the silent admission challenge to an APNs endpoint.
    ///
    /// Only `content-available` keeps the notification service extension out of
    /// it, so the challenge reaches the app.
    async fn send_apple_challenge(
        &self,
        device_token: &str,
        session_id: Uuid,
        challenge: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), ChallengeSendError> {
        let Some(apns_state) = &self.apns_state else {
            return Err(ChallengeSendError::PlatformUnavailable);
        };

        let body = json!({
            "aps": { "content-available": 1 },
            "challenge": challenge,
            "sessionId": session_id,
        });

        let expiration = expires_at.timestamp().max(0).unsigned_abs();
        let (status, response) = self
            .post_apns(
                apns_state,
                device_token,
                "background",
                "5",
                expiration,
                body,
            )
            .await
            .map_err(|error| ChallengeSendError::NotAccepted(error.to_string()))?;

        match status {
            StatusCode::OK => Ok(()),
            StatusCode::GONE | StatusCode::BAD_REQUEST => Err(ChallengeSendError::EndpointRejected),
            s => Err(ChallengeSendError::NotAccepted(format!(
                "status {s}: {response}"
            ))),
        }
    }

    /// Sends the silent admission challenge to an FCM endpoint.
    ///
    /// A data-only message, which reaches the messaging service without a
    /// notification being shown.
    async fn send_google_challenge(
        &self,
        device_token: &str,
        session_id: Uuid,
        challenge: &str,
    ) -> Result<(), ChallengeSendError> {
        let Some(fcm_state) = &self.fcm_state else {
            return Err(ChallengeSendError::PlatformUnavailable);
        };

        let (status, response) = self
            .post_fcm(
                fcm_state,
                device_token,
                json!({
                    "challenge": challenge,
                    "sessionId": session_id
                }),
            )
            .await
            .map_err(|error| match error {
                FcmPostError::MissingProjectId => ChallengeSendError::PlatformUnavailable,
                FcmPostError::OAuth(_) | FcmPostError::Network(_) => {
                    ChallengeSendError::NotAccepted(error.to_string())
                }
            })?;

        match status {
            StatusCode::OK => Ok(()),
            StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST => {
                Err(ChallengeSendError::EndpointRejected)
            }
            s => Err(ChallengeSendError::NotAccepted(format!(
                "status {s}: {response}"
            ))),
        }
    }

    /// Posts one APNs request and reads the response.
    ///
    /// The URL is stripped from any transport error, so a failed push cannot log
    /// the device token it carries.
    async fn post_apns(
        &self,
        apns_state: &ApnsState,
        device_token: &str,
        push_type: &'static str,
        priority: &'static str,
        expiration: u64,
        body: Value,
    ) -> Result<(StatusCode, String), ApnsPostError> {
        let token = self
            .issue_apns_jwt()
            .await
            .map_err(|error| ApnsPostError::Jwt(error.to_string()))?;

        let mut headers = HeaderMap::with_capacity(5);
        let bearer = format!("bearer {}", token.jwt)
            .parse()
            .map_err(|_| ApnsPostError::Jwt("the JWT is not a valid header value".to_owned()))?;
        headers.insert(AUTHORIZATION, bearer);
        headers.insert(
            "apns-topic",
            HeaderValue::from_str(&apns_state.topic).map_err(|_| {
                ApnsPostError::Jwt("the APNs topic is not a header value".to_owned())
            })?,
        );
        headers.insert("apns-push-type", HeaderValue::from_static(push_type));
        headers.insert("apns-priority", HeaderValue::from_static(priority));
        headers.insert("apns-expiration", HeaderValue::from(expiration));

        let res = self
            .client
            .post(format!(
                "{}/3/device/{device_token}",
                apns_state.endpoint.trim_end_matches('/')
            ))
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|error| ApnsPostError::Network(error.without_url().to_string()))?;

        let status = res.status();
        Ok((status, res.text().await.unwrap_or_default()))
    }

    /// Posts one FCM request and reads the response.
    async fn post_fcm(
        &self,
        fcm_state: &FcmState,
        device_token: &str,
        data: Value,
    ) -> Result<(StatusCode, String), FcmPostError> {
        let Some(project_id) = fcm_state.service_account.project_id.as_ref() else {
            return Err(FcmPostError::MissingProjectId);
        };

        let bearer_token = self
            .issue_fcm_token("https://oauth2.googleapis.com/token")
            .await
            .map_err(|error| FcmPostError::OAuth(error.to_string()))?;

        let message = json!({
            "message": {
                "token": device_token,
                "data": data,
                "android": { "priority": "HIGH" }
            }
        });

        let res = self
            .client
            .post(format!(
                "https://fcm.googleapis.com/v1/projects/{project_id}/messages:send"
            ))
            .bearer_auth(bearer_token.token())
            .json(&message)
            .send()
            .await
            .map_err(|error| FcmPostError::Network(error.without_url().to_string()))?;

        let status = res.status();
        Ok((status, res.text().await.unwrap_or_default()))
    }
}

fn create_google_jwt_token(
    encoding_key: &EncodingKey,
    client_email: &str,
    iat: u64,
) -> jsonwebtoken::errors::Result<String> {
    let exp = iat + 3600;
    let claims = FcmClaims {
        iss: client_email,
        scope: "https://www.googleapis.com/auth/firebase.messaging",
        aud: "https://oauth2.googleapis.com/token",
        iat,
        exp,
    };
    let header = Header::new(Algorithm::RS256);
    encode(&header, &claims, encoding_key)
}

impl PushNotificationProvider for ProductionPushNotificationProvider {
    async fn push(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        match push_token.operator() {
            PushTokenOperator::Apple => self.push_apple(push_token).await,
            PushTokenOperator::Google => self.push_google(push_token).await,
        }
    }
}

#[async_trait]
impl ChallengeSender for ProductionPushNotificationProvider {
    fn supports(&self, operator: &PushTokenOperator) -> bool {
        match operator {
            PushTokenOperator::Apple => self.apns_state.is_some(),
            PushTokenOperator::Google => self.fcm_state.is_some(),
        }
    }

    async fn send_challenge(
        &self,
        push_token: &PushToken,
        session_id: Uuid,
        challenge: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), ChallengeSendError> {
        match push_token.operator() {
            PushTokenOperator::Apple => {
                self.send_apple_challenge(push_token.token(), session_id, challenge, expires_at)
                    .await
            }
            PushTokenOperator::Google => {
                self.send_google_challenge(push_token.token(), session_id, challenge)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    // spellchecker:off
    const TEST_RSA_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCMFtzXkOw+XPPg
VIfbYMw8HTD/Vt38UBse60ssReCbxXWPHaHyiTSeSwDCrck7LxZTbmjGU6qOHlET
P53XYKqga43I+wL6vTLxiK+6h1UDKHKlqLMmdqVMc0uNkwm2BcZVqq9ScROtV/nX
e6ZvoLTfIaq2evHRrJYl+1+TX9nDhQp8+X/6gFUxvdEPg31B1SBesclkB7wN3N4s
bGAvxVnrnL1cQAJzURC//mlE2mCXp5BqQSal1TFvOgx93LeCcHLskX5BELRzDCBA
g6Fb2b8utg0148xNId/zRgFm1LARh331nXMrsfMFXE7dZB3vZmdNXB6+SZzkC1a1
u0Ym+aGZAgMBAAECggEADx/kR79JLEn9mAuUT+R9ZGOb78Nti9FLvky1xOHF+Fdr
I/CVdKGo3Uq4eixIYLQKn2cZF4l+rWGbS/5XMKLKhS+bgxaLqa3N31ssKtGz5VeD
ejxynB1c5xo/DtnQR3dL5KGdFGPqNZG9Iw1B6MUjZgBE5bb0Hvi4zMC9HsSPVp8g
feP6E96ihv2ZnObJZjGwfi9XOXPEaaYprtDUulmbkzKNh43wmOx46yeww0X32lXw
CibDjd0pGaW2pPfjFw0C2x8anm4R+5H2Nj+t2Uee2qlkyaoi2uDCL5m64SxH8k+B
Sh9W6wR6PVAxN7BAyjjz8LbN03+2nQF+//22WIls8QKBgQDBsilxR7HiUeWtd91A
YL2dSyKMv2BfGI1GyhvTMScUiRN9ZfLWEf5QUBN3nKErWOF3N9iDUKswaXW8O1SV
vbmaoMT7kEUhN2UwJq0QpBupjblt9pHRKJsT6eN5CnBf3ga3fWQbqgCT+eQfKxNV
ow2rOoeDn2YKvBEybcs4WCWp8QKBgQC5JnovHNyuMmiBNaF/DwdlVy2NfLAo7drf
2+Ydc9TB027TR+mNw9qbOItDUWQOBN6yWJBPxOHAs1FwUqYo3wARlCOIvG3xajo+
72zHZaeMVzEvaBk1YRkEtaS+aLuO8IC/wObKN7ri8aza453861Cu4TRdh8iywq5i
S4C7dy4KKQKBgQCCEQkTMHma6DO60IqZ+Fxbi2Cf8sLcGLiFmKImpxL/Dy0vP45Z
gauscpkf8OWpHf4I+E9Dnp/V2ntc8tpR0x0XYG3mH3LMY05njxEX45tPuAOUe8Zf
FU1NifleBkx/k7Ae9uyKRxYsR9mPtHU/REahfKQTFq6G9tL1chTMuSRRgQKBgHJQ
33/XQioL1ZpxkpTwopBfkzCYm+upcEpna10j92j1Mqgg7oMpOgA8mT+nMS+2sglL
xU57MSfZj57aaN0zUseHv6jdLsSv4eaZzYAPs7Ni4mtyyp26pcfSnzUxVRycQeIj
KFwSrMESlrdPcmyGnfpb8gkNnU1CBomKNKGKpFKBAoGBAKPdE/bz64LSS4CSUJuR
VyA49io/gUqSjPpKOiZwvAqiyDrF6Pt1mR67xBfj3SMClDe8NP6x7NgCJbYtjHyf
OwBzOYWm8dOUD9vBtDyWYw3V46cdk2XAPrzy2wHqD0U9b+fW1p7Pmnrz35R+Nwg6
ejFLxFZmAuiyBWPlOnrZlyWh
-----END PRIVATE KEY-----"#;

    const TEST_EC_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgHz3Iva44aHgSx7n0
c5gHRTX9xPNNaAWBZLCP/wIXCn+hRANCAATXcnNCtSV8Qzeep3Ic3vTSyhCowC5G
44VV2EXhUOa4n5RId2nzLFTbmAONqZm2vdhc5YJMd45b1+5jymRA70yy
-----END PRIVATE KEY-----"#;
    // spellchecker:on

    fn create_temp_fcm_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let sa = json!({
            "client_email": "test@example.com",
            "token_uri": "https://oauth2.googleapis.com/token",
            "private_key": TEST_RSA_KEY,
            "project_id": "test-project"
        });
        writeln!(file.as_file_mut(), "{sa}").unwrap();
        file
    }

    #[test]
    fn test_new_with_fcm_only() {
        let fcm_file = create_temp_fcm_file();
        let fcm_settings = Some(FcmSettings {
            path: fcm_file.path().to_path_buf(),
        });

        let provider = ProductionPushNotificationProvider::new(fcm_settings, None).unwrap();

        assert!(provider.fcm_state.is_some());
        assert!(provider.apns_state.is_none());

        let state = provider.fcm_state.unwrap();
        assert_eq!(
            state.service_account.project_id.as_deref().unwrap(),
            "test-project"
        );
    }

    #[test]
    fn test_new_with_apns_only() {
        let mut apns_file = NamedTempFile::new().unwrap();
        writeln!(apns_file, "{TEST_EC_KEY}").unwrap();

        let apns_settings = Some(ApnsSettings {
            privatekeypath: apns_file.path().to_path_buf(),
            keyid: "KEY123".to_string(),
            teamid: "TEAM456".to_string(),
            topic: None,
            endpoint: None,
        });

        let provider = ProductionPushNotificationProvider::new(None, apns_settings).unwrap();

        assert!(provider.fcm_state.is_none());
        assert!(provider.apns_state.is_some());

        let state = provider.apns_state.unwrap();
        assert_eq!(state.key_id, "KEY123");
        assert_eq!(state.team_id, "TEAM456");
    }

    #[test]
    fn test_new_with_both_providers() {
        let fcm_file = create_temp_fcm_file();
        let mut apns_file = NamedTempFile::new().unwrap();
        writeln!(apns_file, "{TEST_EC_KEY}").unwrap();

        let fcm_settings = Some(FcmSettings {
            path: fcm_file.path().to_path_buf(),
        });
        let apns_settings = Some(ApnsSettings {
            privatekeypath: apns_file.path().to_path_buf(),
            keyid: "K".into(),
            teamid: "T".into(),
            topic: None,
            endpoint: None,
        });

        let provider =
            ProductionPushNotificationProvider::new(fcm_settings, apns_settings).unwrap();
        assert!(provider.fcm_state.is_some());
        assert!(provider.apns_state.is_some());
    }

    #[test]
    fn test_new_fails_on_missing_fcm_file() {
        let fcm_settings = Some(FcmSettings {
            path: "non_existent_file.json".into(),
        });

        let result = ProductionPushNotificationProvider::new(fcm_settings, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_none_settings() {
        let provider = ProductionPushNotificationProvider::new(None, None).unwrap();
        assert!(provider.fcm_state.is_none());
        assert!(provider.apns_state.is_none());
    }

    async fn setup_fcm_provider() -> ProductionPushNotificationProvider {
        let mut fcm_file = NamedTempFile::new().unwrap();
        let sa = json!({
            "client_email": "test-fcm@example.com",
            "token_uri": "http://localhost/ignored",
            "private_key": TEST_RSA_KEY, // Use your existing TEST_KEY const
            "project_id": "test-project"
        });
        writeln!(fcm_file, "{sa}").unwrap();

        ProductionPushNotificationProvider::new(
            Some(FcmSettings {
                path: fcm_file.path().to_path_buf(),
            }),
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_issue_fcm_token_success() {
        let mut server = mockito::Server::new_async().await;

        let google_mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "access_token": "mock-google-bearer-token",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                })
                .to_string(),
            )
            .create();

        let provider = setup_fcm_provider().await;

        let result = provider.issue_fcm_token(&server.url()).await;

        assert!(result.is_ok());
        let fcm_token = result.unwrap();
        assert_eq!(fcm_token.token(), "mock-google-bearer-token");
        google_mock.assert(); // Ensure the HTTP call actually happened
    }

    #[tokio::test]
    async fn test_issue_fcm_token_error_handling() {
        let mut server = mockito::Server::new_async().await;

        let google_mock = server
            .mock("POST", "/")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "error": "invalid_grant",
                    "error_description": "Invalid JWT Signature"
                })
                .to_string(),
            )
            .create();

        let provider = setup_fcm_provider().await;
        let result = provider.issue_fcm_token(&server.url()).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid_grant"));
        assert!(err_msg.contains("Invalid JWT Signature"));
        google_mock.assert(); // Ensure the HTTP call actually happened
    }

    fn setup_provider_with_apns() -> ProductionPushNotificationProvider {
        let mut apns_file = NamedTempFile::new().unwrap();
        writeln!(apns_file, "{TEST_EC_KEY}").unwrap();

        let apns_settings = Some(ApnsSettings {
            privatekeypath: apns_file.path().to_path_buf(),
            keyid: "KEY123".to_string(),
            teamid: "TEAM456".to_string(),
            topic: None,
            endpoint: None,
        });

        ProductionPushNotificationProvider::new(None, apns_settings).unwrap()
    }

    #[tokio::test]
    async fn test_issue_apns_jwt_success() {
        let provider = setup_provider_with_apns();

        let token = provider.issue_apns_jwt().await.unwrap();
        assert!(!token.jwt.is_empty());

        // Verify JWT structure (header.payload.signature)
        let parts = token.jwt.split('.').count();
        assert_eq!(parts, 3);
    }

    #[tokio::test]
    async fn test_issue_apns_jwt_caching() {
        let provider = setup_provider_with_apns();

        let first_token = provider.issue_apns_jwt().await.unwrap();
        let second_token = provider.issue_apns_jwt().await.unwrap();

        assert_eq!(
            first_token, second_token,
            "Provider should return the cached token if it's not expired"
        );
    }
}
