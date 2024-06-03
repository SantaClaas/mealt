use std::{env, net::Ipv4Addr, time::Duration};
use std::env::VarError;

use crate::auth::authenticated_user::AuthenticatedUser;
use askama_axum::{IntoResponse, Template};
use axum::{extract::State, http::Uri, response::Redirect, routing::get, Form, Router, http};
use axum::http::uri::InvalidUri;
use dotenv::dotenv;
use libsql::{named_params, Builder, Connection, Database};
use serde::{self, Deserialize};
use thiserror::Error;
use tower_http::services::ServeDir;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::auth::USER_HOME_PAGE;
use crate::secrets::Secrets;

mod auth;
mod email;
mod database;
mod routes;
mod secrets;

const GUEST_LIST_URL_KEY: &str = "GUEST_LIST_URL";

#[derive(Debug, Error)]
enum Error {
    #[error("GUEST_LIST_URL environment variable must be set")]
    NoGuestListUrl(#[from] VarError),
    #[error("Error getting secrets")]
    GetSecretsError(#[from] secrets::GetSecretsError),
    #[error("Error serving")]
    ServeError(#[from] std::io::Error),
    #[error("Invalid URL in GUEST_LIST_URL environment variable")]
    InvalidUrl(#[from] InvalidUri),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "guest_list=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

   let secrets = secrets::get_secrets().await?;

    // Set up database
    let connection = database::initialize_database().await;

    // Set up background workers
    let _handle = tokio::spawn(collect_garbage(connection.clone()));

    // Configuration
    //TODO implement fallback to localhost
    //TODO implement warning that users can not follow links (e.g. in emails) if host is localhost or 127.0.0.1
    let url = env::var(GUEST_LIST_URL_KEY).map_err(Error::NoGuestListUrl)?;

    let url = Uri::try_from(url)?;
    let port = url.port().map(|port| port.as_u16()).unwrap_or(80);
    let configuration = Configuration { server_url: url };

    let client = reqwest::Client::new();
    let app_state = AppState {
        connection,
        client,
        configuration,
        secrets,
    };

    let auth_routes = auth::create_router();

    // Build our application with a route
    let app = Router::new()
        .route("/", get(routes::index::get_page))
        .route(USER_HOME_PAGE, get(get_apps_page))
        .merge(auth_routes)
        // If the route could not be matched it might be a file
        .fallback_service(ServeDir::new("public"))
        .with_state(app_state);

    // Run the server
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), port))
        .await
        .unwrap();

    tracing::debug!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;

    Ok(())
}


#[derive(Clone)]
pub(crate) struct Configuration {
    /// The server URL under which the server can be reached publicly for clients.
    /// A user clicking an email link will be brought to this URL.
    server_url: Uri,
}


#[derive(Clone)]
pub(crate) struct AppState {
    connection: Connection,
    client: reqwest::Client,
    configuration: Configuration,
    secrets: Secrets,
}


#[derive(Template)]
#[template(path = "apps.html")]
struct AppsTemplate {}

async fn get_apps_page(
    user: AuthenticatedUser,
    State(app_state): State<AppState>,
) -> impl IntoResponse {

    let apps_template = AppsTemplate {  };
    apps_template
}

/// Runs forever and cleans up expired app data about every 5 minutes
async fn collect_garbage(connection: Connection) {
    // It is not important that it cleans exactly every 5 minutes, but it is important that it happens regularly
    // Duration from minutes is experimental currently
    let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
    loop {
        interval.tick().await;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();

    }
}
