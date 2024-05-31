mod command;

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

    #[error("Error merging pending commit")]
    MergePendingCommitError(
        #[from]
        #[serde(skip)]
        MergePendingCommitError<MemoryKeyStoreError>,
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

    let (_mls_message_out, welcome_out, _group_information) =
        group.add_members(state.backend.as_ref(), &user.signature_key, &[package])?;

    //TODO check if commit needs to be synchronized with others
    // Merge pending commit that adds the new member
    group.merge_pending_commit(state.backend.as_ref())?;

    // Return welcome message to frontend to send over websockets to other clients
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

    //TODO Private message errors
    #[error("Group not found")]
    GroupNotFound,
    #[error("Error processing message")]
    ProcessMessageError(
        #[from]
        #[serde(skip)]
        ProcessMessageError,
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

    let extract = message.extract();
    match extract {
        MlsMessageInBody::Welcome(welcome) => {
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
            Ok(())
        },

        MlsMessageInBody::PrivateMessage(message) => {
            // Process the message
            let protocol_message : ProtocolMessage = message.into();

            let id = protocol_message.group_id();
            let id = BASE64_URL_SAFE_NO_PAD.encode(id.as_slice());

            let mut groups = state.groups.lock().await;
            let Some(group) = groups.get_mut(&id) else {
                return Err(ReceiveMessageError::GroupNotFound);
            };

            let processed_message = group.process_message(state.backend.as_ref(), protocol_message)?;
            match processed_message.into_content() {
                ProcessedMessageContent::ApplicationMessage(application_message) => {
                    //TODO send message to frontend
                    println!("Application message: {:?}", application_message);
                },
                _ => unimplemented!("Message processed but that type is not implemented yet"),
            }
            Ok(())
        },
        _ => unimplemented!("Processing messages is not implemented yet"),
    }

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

#[tauri::command]
async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, ()> {
    let groups = state.groups.lock().await;

    let ids = groups.keys().cloned().collect();

    Ok(ids)
}

#[derive(Error, Debug, Serialize)]
enum GetIdentityError {
    #[error("No user is signed in")]
    NoUserError,
}

#[tauri::command]
async fn get_identity(state: State<'_, AppState>) -> Result<String, GetIdentityError> {
    let user = state.user.lock().await;
    let Some(user) = user.as_ref() else {
        return Err(GetIdentityError::NoUserError);
    };

    let id = user.credential.credential.identity();
    let id = BASE64_URL_SAFE_NO_PAD.encode(id);
    Ok(id)
}

#[derive(Error, Debug, Serialize)]
enum CreateMessageError {
    #[error("No user is signed in")]
    NoUserError,
    #[error("Group not found")]
    GroupNotFound,
    #[error("Error creating message")]
    CreateMessageError(
        #[from]
        #[serde(skip)]
        openmls::group::CreateMessageError,
    ),
    #[error("Error serializing message")]
    SerializeMessageError(
        #[from]
        #[serde(skip)]
        tls_codec::Error,
    ),
}

#[tauri::command]
async fn create_message(
    state: State<'_, AppState>,
    group_id: &str,
    message: &str,
) -> Result<Vec<u8>, CreateMessageError> {
    let user = state.user.lock().await;
    let Some(user) = user.as_ref() else {
        return Err(CreateMessageError::NoUserError);
    };

    let mut groups = state.groups.lock().await;
    let Some(group) = groups.get_mut(group_id) else {
        return Err(CreateMessageError::GroupNotFound);
    };

    let message = group.create_message(
        state.backend.as_ref(),
        &user.signature_key,
        message.as_bytes(),
    )?;

    let data = message.tls_serialize_detached()?;
    Ok(data)
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
            create_group,
            create_message,
            command::create_user,
            is_authenticated,
            get_groups,
            get_identity,
            invite_package,
            process_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
