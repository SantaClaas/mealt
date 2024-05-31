use base64::prelude::*;
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{MemoryKeyStoreError, OpenMlsRustCrypto};
use reqwest::{Client, Method};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, PoisonError},
};
use tauri::{AppHandle, Manager, State};
use thiserror::Error;
use tokio::sync::Mutex;
// Disable dead code warnings for this file
#[allow(dead_code)]
pub(crate) const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct User {
    credential: CredentialWithKey,
    signature_key: SignatureKeyPair,
}

struct AppState {
    backend: Arc<OpenMlsRustCrypto>,
    user: Arc<Mutex<Option<User>>>,
    groups: Arc<Mutex<HashMap<String, MlsGroup>>>,
    client: Client,
}

#[derive(Error, Debug, Serialize)]
enum CreateUserError {
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
async fn create_user(name: &str, state: State<'_, AppState>) -> Result<(), CreateUserError> {
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
async fn is_authenticated(state: State<'_, AppState>) -> Result<bool, IsAuthenticatedError> {
    let state = state.user.lock().await;
    Ok(state.is_some())
}

#[derive(Error, Debug, Serialize)]
enum CreateGroupError {
    #[error("No user is signed in")]
    NoUserError,
    #[error("Error creating group")]
    NewGroupError(
        #[from]
        #[serde(skip)]
        NewGroupError<MemoryKeyStoreError>,
    ),
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
    #[error("No user is signed in")]
    NoUserError,
    #[error("Error advertising key package")]
    AdvertiseKeyPackageError(#[from] AdvertiseKeyPackageError),
}

#[tauri::command]
async fn advertise(state: State<'_, AppState>) -> Result<(), AdvertiseError> {
    let user = state.user.lock().await;
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

    Ok(())
}
#[derive(Error, Debug, Serialize)]
enum GetPackageError {
    #[error("Error getting package from server")]
    RequestError(
        #[from]
        #[serde(skip)]
        reqwest::Error,
    ),
    #[error("Error deserializing package")]
    DeserializeError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),
}

async fn get_package(id: &str, client: &Client) -> Result<KeyPackageIn, GetPackageError> {
    let response = client
        .get(&format!("http://localhost:3000/packages/{}", id))
        .send()
        .await?
        .error_for_status()?;

    //TODO directly stream bytes into deserializer without first buffering
    let bytes = response.bytes().await?;
    let package = KeyPackageIn::tls_deserialize(&mut bytes.as_ref())?;

    Ok(package)
}

#[derive(Error, Debug, Serialize)]
enum SendMessageError {
    #[error("Error sending message")]
    RequestError(
        #[from]
        #[serde(skip)]
        reqwest::Error,
    ),
    #[error("Error serializing message")]
    SerializeError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),
}

async fn send_message(
    recipient: String,
    message: MlsMessageOut,
    client: &Client,
) -> Result<(), SendMessageError> {
    let message = message.tls_serialize_detached()?;
    let response = client
        .post(&format!("http://localhost:3000/messages/{}", recipient))
        .body(message)
        .send()
        .await?;

    response.error_for_status()?;
    Ok(())
}

#[derive(Error, Debug, Serialize)]
enum InvitePackageError {
    #[error("No user is signed in")]
    NoUserError,
    #[error("Group not found")]
    GroupNotFound,
    #[error("Error getting package from server")]
    GetPackageError(
        #[from]
        #[serde(skip)]
        GetPackageError,
    ),
    #[error("Error validating package")]
    ValidatePackageError(
        #[from]
        #[serde(skip)]
        KeyPackageVerifyError,
    ),

    #[error("Error adding member to group")]
    AddMemberError(
        #[from]
        #[serde(skip)]
        AddMembersError<MemoryKeyStoreError>,
    ),

    #[error("Error serializing welcome message")]
    SerializeWelcomeError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),
}

#[tauri::command]
async fn invite_package(
    group_id: &str,
    package_id: &str,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, InvitePackageError> {
    let user = state.user.lock().await;
    let Some(user) = user.as_ref() else {
        return Err(InvitePackageError::NoUserError);
    };

    let mut groups = state.groups.lock().await;
    let Some(group) = groups.get_mut(group_id) else {
        return Err(InvitePackageError::GroupNotFound);
    };

    let package = get_package(package_id, &state.client).await?;

    let backend = state.backend.crypto();
    let package = package.validate(backend, ProtocolVersion::default())?;

    let (mls_message_out, welcome_out, group_information) =
        group.add_members(state.backend.as_ref(), &user.signature_key, &[package])?;

    // Return welcome message to frontent to send over websockets to other clients
    let data = welcome_out.tls_serialize_detached()?;

    Ok(data)
}

#[derive(Error, Debug, Serialize)]
enum ReceiveMessageError {
    #[error("Error deserializing message")]
    DeserializeError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),
    #[error("Error joining group")]
    JoinGroupError(
        #[from]
        #[serde(skip)]
        WelcomeError<MemoryKeyStoreError>,
    ),
    #[error("Error emitting event")]
    EmitError(
        #[from]
        #[serde(skip)]
        tauri::Error,
    ),
}

const JOIN_GROUP_EVENT: &str = "join_group";

#[derive(Serialize, Clone)]
struct JoinGroupEvent {
    group_id: String,
}
#[tauri::command]
async fn process_message(
    data: Vec<u8>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), ReceiveMessageError> {
    let message = MlsMessageIn::tls_deserialize(&mut data.as_slice())?;

    if let MlsMessageInBody::Welcome(welcome) = message.extract() {
        // Create group from welcome message
        let group = MlsGroup::new_from_welcome(
            state.backend.as_ref(),
            &MlsGroupConfig::default(),
            welcome,
            None,
        )?;

        let id = group.group_id();
        let id = BASE64_URL_SAFE_NO_PAD.encode(id.as_slice());

        let mut groups = state.groups.lock().await;
        groups.insert(id.clone(), group);

        app.emit(JOIN_GROUP_EVENT, JoinGroupEvent { group_id: id })?;
        return Ok(());
    }

    todo!()
}

#[tauri::command]
async fn create_group(state: State<'_, AppState>) -> Result<String, CreateGroupError> {
    let user = state.user.lock().await;
    let Some(user) = user.as_ref() else {
        return Err(CreateGroupError::NoUserError);
    };

    let group_configuration = MlsGroupConfigBuilder::default()
        .use_ratchet_tree_extension(true)
        .build();

    let group = MlsGroup::new(
        &*state.backend,
        &user.signature_key,
        &group_configuration,
        user.credential.clone(),
    )?;

    let id = group.group_id().as_slice();
    let id = BASE64_URL_SAFE_NO_PAD.encode(id);

    let mut groups = state.groups.lock().await;
    groups.insert(id.clone(), group);

    Ok(id)
}

//TODO check if I can contribute support for Infallible error to tauri
#[tauri::command]
async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, ()> {
    let groups = state.groups.lock().await;

    let ids = groups.keys().cloned().collect();

    Ok(ids)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = Client::new();
    let state = AppState {
        backend: OpenMlsRustCrypto::default().into(),
        user: Arc::default(),
        groups: Arc::default(),
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
            invite_package,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
