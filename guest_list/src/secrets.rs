// 🤫

use std::sync::Arc;
use bitwarden::auth::login::AccessTokenLoginRequest;
use bitwarden::Client;
use bitwarden::secrets_manager::secrets::{SecretGetRequest, SecretIdentifiersRequest, SecretIdentifiersResponse};
use dotenv::dotenv;
use thiserror::Error;
use uuid::{Uuid, uuid};

#[derive(Debug, Error)]
pub(super)enum GetSecretsError {
    #[cfg(debug_assertions)]
    #[error("Error loading dotenv file")]
    DotEnvError(#[from] dotenv::Error),
    #[error("No Bitwarden Secrets Manager token in environment")]
    NoBwsToken(std::env::VarError),
    #[error("Error getting secrets from Bitwarden Secrets Manager")]
    BwsError(#[from] bitwarden::error::Error),
    #[error("No id for Resend API key found")]
    NoResendSecretId(std::env::VarError),
    #[error("Error parsing secret id. Is it a valid UUID?")]
    InvalidSecretId(#[from] uuid::Error),

}


#[derive(Clone)]
pub(crate) struct Secrets {
    pub(crate) resend_auth_token: Arc<str>,
}

pub(super) async fn get_secrets() -> Result<Secrets, GetSecretsError> {
    // Use default settings
    let mut client = Client::new(None);

    // Set up machine account token
    #[cfg(debug_assertions)]
    {
        // Use .env files only for debug convenience
        let result = dotenv();
        if let Err(error) = result {
            tracing::warn!("No dotenv loaded in debug mode: {}", error);
        }
    }

    let token = std::env::var("BWS_TOKEN").map_err(GetSecretsError::NoBwsToken)?;
    let token = AccessTokenLoginRequest { access_token: token, state_file: None };
    client.auth().login_access_token(&token).await?;

    // Ids are not a secret but should still be avoided to be shared where possible (obfuscation)
    let resend_secret_id = std::env::var("BWS_RESEND_SECRET_ID").map_err(GetSecretsError::NoResendSecretId)?.parse::<Uuid>()?;
    let request = SecretGetRequest{
        id: resend_secret_id,
    };

    let secret = client.secrets().get(&request).await.unwrap();

    let secrets = Secrets {
        resend_auth_token: secret.value.into(),
    };

    Ok(secrets)
}

