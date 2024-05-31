use openmls::credentials::{Credential, CredentialType, CredentialWithKey};
use openmls::prelude::CredentialError;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::types::CryptoError;
use serde::Serialize;
use tauri::State;
use thiserror::Error;
use crate::{AdvertiseKeyPackageError, AppState, CIPHERSUITE, User};

#[derive(Error, Debug, Serialize)]
pub(crate) enum CreateUserError {
    #[error("User already exists")]
    UserExists,
    #[error("Error creating credentials for user")]
    CredentialsError(
        #[from]
        #[serde(skip)]
        CredentialError,
    ),

    #[error("Error creating signature key pair")]
    SignatureKeyPairError(
        #[from]
        #[serde(skip)]
        CryptoError,
    ),

    #[error("Error advertising key package on server")]
    AdvertiseKeyPackageError(
        #[from]
        #[serde(skip)]
        AdvertiseKeyPackageError,
    ),
}

#[tauri::command]
pub(crate) async fn create_user(name: &str, state: State<'_, AppState>) -> Result<(), CreateUserError> {
    let mut state = state.user.lock().await;
    if state.is_some() {
        return Err(CreateUserError::UserExists);
    }

    let credential = Credential::new(name.into(), CredentialType::Basic)?;
    let signature_key_pair = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;

    let credential = CredentialWithKey {
        credential,
        signature_key: signature_key_pair.public().into(),
    };

    let user = User {
        credential,
        signature_key: signature_key_pair,
    };

    state.replace(user);

    Ok(())
}
