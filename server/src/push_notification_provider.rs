// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{
    qs::{PushNotificationError, PushNotificationProvider},
    settings::{ApnsSettings, FcmSettings},
};
use aircommon::messages::push_token::{PushToken, PushTokenOperator};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::File,
    io::Read,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zeroize::Zeroize;

#[derive(Debug, Serialize)]
struct FcmClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: usize,
    exp: usize,
}

// Struct for the Google OAuth2 response
#[derive(Debug, Deserialize)]
struct OauthSuccessResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct OauthErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApnsClaims {
    iss: String,
    iat: usize,
}

#[derive(Debug, Clone)]
struct ApnsToken {
    jwt: String,
    issued_at: u64,
}

#[derive(Debug, Clone, Zeroize)]
struct FcmToken {
    token: String,
    expires_at: u64, // Seconds since UNIX_EPOCH
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

#[derive(Debug, Clone)]
struct FcmState {
    service_account: ServiceAccount,
    token: Arc<Mutex<Option<FcmToken>>>,
}

#[derive(Debug, Clone)]
pub struct ApnsState {
    pub key_id: String,
    pub team_id: String,
    pub private_key: Vec<u8>,
    token: Arc<Mutex<Option<ApnsToken>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize)]
pub struct ServiceAccount {
    #[serde(rename = "type")]
    pub key_type: Option<String>,
    pub project_id: Option<String>,
    pub private_key_id: Option<String>,
    pub private_key: String,
    pub client_email: String,
    pub client_id: Option<String>,
    pub auth_uri: Option<String>,
    pub token_uri: String,
    pub auth_provider_x509_cert_url: Option<String>,
    pub client_x509_cert_url: Option<String>,
    pub universe_domain: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProductionPushNotificationProvider {
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
            let service_account = std::fs::read_to_string(fcm_settings.path)?;

            Some(FcmState {
                service_account: serde_json::from_str(&service_account)?,
                token: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        };

        // Read the parameters for APNS
        let apns_state = if let Some(apns_settings) = apns_settings {
            // Read the private key
            let mut private_key_file = File::open(&apns_settings.privatekeypath)?;
            let mut private_key_p8 = String::new();
            private_key_file.read_to_string(&mut private_key_p8)?;

            Some(ApnsState {
                key_id: apns_settings.keyid,
                team_id: apns_settings.teamid,
                private_key: private_key_p8.into_bytes(),
                token: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        };

        Ok(Self {
            fcm_state,
            apns_state,
        })
    }

    async fn issue_fcm_token(&self) -> Result<FcmToken, Box<dyn std::error::Error + Send + Sync>> {
        // TODO #237: Proactively refresh the token before it expires
        let fcm_state = self.fcm_state.as_ref().ok_or("Missing Service Account")?;

        // Check whether we already have a token and if it is still valid
        let mut token_option = fcm_state.token.lock().await;
        if let Some(token) = token_option.as_ref()
            && !token.is_expired()
        {
            return Ok(token.clone());
        }

        let service_account = &fcm_state.service_account;

        // Extract necessary fields from the service account
        let private_key = &service_account.private_key;
        let client_email = &service_account.client_email;

        // Generate JWT claims
        let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as usize;
        let exp = iat + 3600; // Token valid for 1 hour

        let claims = FcmClaims {
            iss: client_email.to_string(),
            scope: "https://www.googleapis.com/auth/firebase.messaging".to_string(),
            aud: "https://oauth2.googleapis.com/token".to_string(),
            iat,
            exp,
        };

        // Create the JWT
        let header = Header::new(Algorithm::RS256);
        let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())?;
        let jwt = encode(&header, &claims, &encoding_key)?;

        // Send the JWT to Google's OAuth2 token endpoint and get a bearer token
        // back
        let client = Client::new();
        let response = client
            .post("https://oauth2.googleapis.com/token")
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
    async fn issue_apns_jwt(&self) -> Result<String, Box<dyn std::error::Error>> {
        // TODO #237: Proactively refresh the jwt before it expires
        let apns_state = self.apns_state.as_ref().ok_or("Missing ApnsState")?;

        // Check whether we already have a token and if it is still valid, i.e.
        // not older than 40 minutes
        let mut token_option = apns_state.token.lock().await;

        if let Some(token) = &*token_option {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if now < token.issued_at + 60 * 40 {
                return Ok(token.jwt.clone());
            }
        }
        // Generate the current time in seconds since UNIX_EPOCH
        let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as usize;

        // Create the JWT claims
        let claims = ApnsClaims {
            iss: apns_state.team_id.clone(),
            iat,
        };

        // Create the JWT header
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(apns_state.key_id.clone());

        // Encode the JWT
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_ec_pem(&apns_state.private_key)?,
        )?;

        // Store the JWT and update the last issuance time
        *token_option = Some(ApnsToken {
            jwt: token.clone(),
            issued_at: iat as u64,
        });

        Ok(token)
    }

    async fn push_google(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        // If we don't have an FCM state, we can't send push notifications
        let Some(fcm_state) = &self.fcm_state else {
            return Ok(());
        };

        let service_account = &fcm_state.service_account;

        let bearer_token = self
            .issue_fcm_token()
            .await
            .map_err(|e| PushNotificationError::OAuthError(e.to_string()))?;

        // Extract the project ID from the service account
        let Some(ref project_id) = service_account.project_id else {
            return Err(PushNotificationError::InvalidConfiguration(
                "Missing project ID in service account".to_string(),
            ));
        };

        // Create the URL
        let url = format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send");

        // Construct the message payload
        let message = json!({
            "message": {
                "token": push_token.token(),
                "data": {
                    "data": "",
                },
                "android": {
                    "priority": "HIGH",
                }
            }
        });

        // Send the request
        let client = Client::new();
        let res = client
            .post(&url)
            .bearer_auth(bearer_token.token())
            .json(&message)
            .send()
            .await
            .map_err(|e| PushNotificationError::NetworkError(e.to_string()))?;

        match res.status() {
            StatusCode::OK => Ok(()),
            // If the token is invalid, we might want to know it and
            // delete it
            StatusCode::NOT_FOUND => Err(PushNotificationError::InvalidToken(
                res.text().await.unwrap_or_default(),
            )),
            // If the status code is not OK or NOT_FOUND, we might want to
            // log the error
            s => Err(PushNotificationError::Other(format!(
                "Unexpected status code: {} with body: {}",
                s,
                res.text().await.unwrap_or_default()
            ))),
        }
    }

    async fn push_apple(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        // If we don't have an APNS state, we can't send push notifications
        if self.apns_state.is_none() {
            return Ok(());
        }

        // Issue the JWT
        let jwt = self
            .issue_apns_jwt()
            .await
            .map_err(|e| PushNotificationError::JwtCreationError(e.to_string()))?;

        // Create the URL
        let url = format!(
            "https://api.push.apple.com:443/3/device/{}",
            push_token.token()
        );

        // Create the headers and payload
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("authorization", format!("bearer {jwt}").parse().unwrap());
        headers.insert("apns-topic", "ms.air".parse().unwrap());
        headers.insert("apns-push-type", "alert".parse().unwrap());
        headers.insert("apns-priority", "10".parse().unwrap());
        headers.insert("apns-expiration", "0".parse().unwrap());

        let body = r#"
        {
            "aps": {
                "alert": {
                "title": "Empty notification",
                "body": "This artefact should disappear once the app is in public beta."
                },
                 "mutable-content": 1
            },
            "data": "data",
        }
        "#;

        // Send the push notification
        let client = Client::new();
        let res = client
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| PushNotificationError::NetworkError(e.to_string()))?;

        match res.status() {
            StatusCode::OK => Ok(()),
            // If the token is invalid, we might want to know it and
            // delete it
            StatusCode::GONE => Err(PushNotificationError::InvalidToken(
                res.text().await.unwrap_or_default(),
            )),
            // If the status code is not OK or GONE, we might want to
            // log the error
            s => Err(PushNotificationError::Other(format!(
                "Unexpected status code: {} with body: {}",
                s,
                res.text().await.unwrap_or_default()
            ))),
        }
    }
}

impl PushNotificationProvider for ProductionPushNotificationProvider {
    async fn push(&self, push_token: PushToken) -> Result<(), PushNotificationError> {
        match push_token.operator() {
            PushTokenOperator::Apple => self.push_apple(push_token).await,
            PushTokenOperator::Google => self.push_google(push_token).await,
        }
    }
}
