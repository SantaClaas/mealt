use std::env;
use libsql::{Builder, Connection, Database};

// Use local database for debugging
#[cfg(debug_assertions)]
async fn create_database() -> Database {
    Builder::new_local("database.db").build().await.unwrap()
}

#[cfg(not(debug_assertions))]
async fn create_database() -> Database {

    let url =  todo!("Set up loading secrets from Bitwarden");
    let token =  todo!("Set up loading secrets from Bitwarden");

    Builder::new_remote(url, token)
        .build()
        .await
        .expect("Failed to connect to database")
}

/// Creates the database and initializes it with the tables
pub(super) async fn initialize_database() -> Connection {

    let database = create_database().await;

    let connection = database.connect().unwrap();

    let query = include_str!("./create_tables.sql");
    connection
        .execute_batch(&query)
        .await
        .expect("Failed to create tables");

    tracing::debug!("Tables created");

    connection
}