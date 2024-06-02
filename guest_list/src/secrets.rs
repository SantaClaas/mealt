// 🤫

use bitwarden::auth::login::AccessTokenLoginRequest;
use bitwarden::Client;
use bitwarden::secrets_manager::secrets::{SecretIdentifiersRequest, SecretIdentifiersResponse};
use dotenv::dotenv;
use thiserror::Error;
use uuid::{Uuid, uuid};

#[derive(Debug, Error)]
pub(super)enum GetSecretsError {
    #[error("Error loading dotenv file")]
    DotEnvError(#[from] dotenv::Error),
    #[error("No BWS token in environment")]
    NoBwsToken(std::env::VarError),
    #[error("No BWS organization id in environment")]
    NoBwsOrganizationId(std::env::VarError),
    #[error("Error parsing BWS organization id. Is it a valid UUID?")]
    InvalidBwsOrganizationId(#[from] uuid::Error),
    #[error("Error getting secrets from BWS")]
    BwsError(#[from] bitwarden::error::Error),
}

pub(super) async fn get_secrets() -> Result<SecretIdentifiersResponse, GetSecretsError> {
    // Use default settings
    let mut client = Client::new(None);

    // Set up machine account token
    #[cfg(debug_assertions)]
    {
        let result = dotenv();
        if result.is_err() {
            tracing::warn!("No dotenv loaded in debug mode");
        }

    }

    let token = std::env::var("BWS_TOKEN").map_err(GetSecretsError::NoBwsToken)?;
    let token = AccessTokenLoginRequest { access_token: token, state_file: None };
    client.auth().login_access_token(&token).await?;

    let organization_id = std::env::var("BWS_ORGANIZATION_ID").map_err(GetSecretsError::NoBwsOrganizationId)?.parse::<Uuid>()?;
    let identifier = SecretIdentifiersRequest {
        organization_id,
    };
    let secrets = client.secrets().list(&identifier).await?;

    Ok(secrets)
}

