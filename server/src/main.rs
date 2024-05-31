use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{ws::WebSocket, Path, State, WebSocketUpgrade},
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::prelude::*;
use key_package::KeyPackage;
use mls_message::MlsMessage;
use openmls::prelude::*;
use openmls::{key_packages::KeyPackageIn, prelude::TlsSerializeTrait};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use user_actor::UserActorHandle;

mod key_package;
mod mls_message;
mod user_actor;
mod websocket_actor;

#[derive(Clone)]
struct AppState {
    key_packages_by_identity: Arc<Mutex<HashMap<String, KeyPackageIn>>>,
    user_actors: Arc<Mutex<Vec<UserActorHandle>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route(
            "/packages",
            get(get_key_package_identities).post(create_key_package),
        )
        .route("/packages/:identity", get(get_key_package))
        .route("/:identity/messages", get(websocket_handler))
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                // .allow_origin("localhost:1420".parse::<HeaderValue>().unwrap())
                // .allow_origin("localhost:1421".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET]),
        )
        .with_state(AppState {
            key_packages_by_identity: Default::default(),
            user_actors: Default::default(),
        });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn create_key_package(
    State(state): State<AppState>,
    KeyPackage(package): KeyPackage,
) -> Result<(), StatusCode> {
    tracing::debug!("Received key package");
    let mut key_packages = state.key_packages_by_identity.lock().await;

    let credential = package.unverified_credential().credential;
    let identity = credential.identity();

    let Ok(identity) = std::str::from_utf8(identity) else {
        return Err(StatusCode::BAD_REQUEST);
    };

    key_packages.insert(identity.to_string(), package);
    Ok(())
}

async fn get_key_package_identities(State(state): State<AppState>) -> impl IntoResponse {
    let key_packages = state.key_packages_by_identity.lock().await;
    let identities = key_packages.keys().cloned().collect::<Vec<_>>();

    Json(identities)
}

async fn get_key_package(
    State(state): State<AppState>,
    Path(identity): Path<String>,
) -> Result<Vec<u8>, StatusCode> {
    let packages = state.key_packages_by_identity.lock().await;
    let Some(package) = packages.get(&identity) else {
        return Err(StatusCode::NOT_FOUND);
    };

    package
        .tls_serialize_detached()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn websocket_handler(
    Path(identity): Path<String>,
    websocket: WebSocketUpgrade,
    state: State<AppState>,
) -> impl IntoResponse {
    websocket.on_upgrade(move |socket| create_actor(socket, state, identity))
}

// 2/3e, duck2duck encryption, melt
async fn create_actor(stream: WebSocket, State(state): State<AppState>, identity: String) {
    let actor = UserActorHandle::new(identity, stream, state.user_actors.clone());
    let mut actors_guild = state.user_actors.lock().await;
    actors_guild.push(actor);
}
