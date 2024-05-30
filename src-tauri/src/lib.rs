use base64::prelude::*;
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{MemoryKeyStore, MemoryKeyStoreError, OpenMlsRustCrypto};
use reqwest::{Client, Method};
use serde::Serialize;
use std::sync::{Mutex, PoisonError};
use tauri::{Manager, State};
use thiserror::Error;
// Disable dead code warnings for this file
#[allow(dead_code)]
pub(crate) const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct User {
    credential: CredentialWithKey,
    signature_key: SignatureKeyPair,
}

struct AppState {
    backend: OpenMlsRustCrypto,
    user: Mutex<Option<User>>,
    groups: Mutex<Vec<MlsGroup>>,
    client: Client,
}

#[derive(Error, Debug, Serialize)]
enum CreateUserError {
    #[error("Could not access state")]
    PoisonError(),
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

impl<T> From<PoisonError<T>> for CreateUserError {
    fn from(_: PoisonError<T>) -> Self {
        CreateUserError::PoisonError()
    }
}

#[tauri::command]
fn create_user(name: &str, state: State<AppState>) -> Result<(), CreateUserError> {
    let mut state = state.user.lock()?;
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

#[derive(Error, Debug, Serialize)]
enum IsAuthenticatedError {
    #[error("Could not access state")]
    PoisonError,
}

impl<T> From<PoisonError<T>> for IsAuthenticatedError {
    fn from(_: PoisonError<T>) -> Self {
        IsAuthenticatedError::PoisonError
    }
}

#[tauri::command]
fn is_authenticated(state: State<AppState>) -> Result<bool, IsAuthenticatedError> {
    let state = state.user.lock()?;
    Ok(state.is_some())
}

#[derive(Error, Debug, Serialize)]
enum CreateGroupError {
    #[error("Could not access state")]
    PoisonError,
    #[error("No user is signed in")]
    NoUserError,
    #[error("Error creating group")]
    NewGroupError(
        #[from]
        #[serde(skip)]
        NewGroupError<MemoryKeyStoreError>,
    ),
    #[error("Could not access groups")]
    GroupsPoisonError,
}

#[derive(Error, Debug, Serialize)]
enum AdvertiseKeyPackageError {
    #[error("Could not create a key package")]
    CreateKeyPackageError(
        #[from]
        #[serde(skip)]
        KeyPackageNewError<MemoryKeyStoreError>,
    ),
    #[error("Could not serialize key package")]
    SerializeKeyPackageError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),

    #[error("Could not send key package")]
    RequestError(
        #[from]
        #[serde(skip)]
        reqwest::Error,
    ),
}

async fn advertise_key_package(
    backend: &OpenMlsRustCrypto,
    signer: &SignatureKeyPair,
    credential_with_key: CredentialWithKey,
    client: &Client,
) -> Result<(), AdvertiseKeyPackageError> {
    // Create key package
    let package = KeyPackage::builder().build(
        CryptoConfig {
            ciphersuite: CIPHERSUITE,
            version: ProtocolVersion::default(),
        },
        backend,
        signer,
        credential_with_key,
    )?;

    let package = package.tls_serialize_detached()?;
    let response = client
        .request(Method::POST, "http://localhost:3000/packages")
        .body(package)
        .send()
        .await?;

    response.error_for_status()?;
    Ok(())
}

#[derive(Error, Debug, Serialize)]
enum AdvertiseError {
    #[error("Could not access state")]
    PoisonError,
    #[error("No user is signed in")]
    NoUserError,
    #[error("Error advertising key package")]
    AdvertiseKeyPackageError(#[from] AdvertiseKeyPackageError),
}

impl<T> From<PoisonError<T>> for AdvertiseError {
    fn from(_: PoisonError<T>) -> Self {
        AdvertiseError::PoisonError
    }
}

#[tauri::command]
async fn advertise(state: State<'_, AppState>) -> Result<(), AdvertiseError> {
    let user = state.user.lock().map_err(|_| AdvertiseError::PoisonError)?;
    let Some(user) = user.as_ref() else {
        return Err(AdvertiseError::NoUserError);
    };

    advertise_key_package(
        &state.backend,
        &user.signature_key,
        user.credential.clone(),
        &state.client,
    )
    .await?;
    todo!()
}

#[tauri::command]
fn create_group(state: State<AppState>) -> Result<String, CreateGroupError> {
    let user = state
        .user
        .lock()
        .map_err(|_| CreateGroupError::PoisonError)?;
    let Some(user) = user.as_ref() else {
        return Err(CreateGroupError::NoUserError);
    };

    let group_configuration = MlsGroupConfigBuilder::default()
        .use_ratchet_tree_extension(true)
        .build();

    let group = MlsGroup::new(
        &state.backend,
        &user.signature_key,
        &group_configuration,
        user.credential.clone(),
    )?;

    let id = group.group_id().as_slice();
    let id = BASE64_URL_SAFE_NO_PAD.encode(id);

    let mut groups = state
        .groups
        .lock()
        .map_err(|_| CreateGroupError::GroupsPoisonError)?;

    groups.push(group);

    Ok(id)
}

#[derive(Error, Debug, Serialize)]
enum GetGroupsError {
    #[error("Could not access state")]
    PoisonError,
}

#[tauri::command]
fn get_groups(state: State<AppState>) -> Result<Vec<String>, GetGroupsError> {
    let groups = state
        .groups
        .lock()
        .map_err(|_| GetGroupsError::PoisonError)?;

    Ok(groups
        .iter()
        .map(|groups| BASE64_URL_SAFE_NO_PAD.encode(groups.group_id().as_slice()))
        .collect())

    // todo!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = Client::new();
    let state = AppState {
        backend: OpenMlsRustCrypto::default(),
        user: Mutex::new(None),
        groups: Mutex::new(Vec::new()),
        client,
    };

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)] // only include this code on debug builds
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
                window.close_devtools();
            }
            Ok(())
        })
        .manage(state)
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            advertise,
            is_authenticated,
            create_group,
            create_user,
            get_groups,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
