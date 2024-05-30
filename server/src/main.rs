use axum::{response::Html, routing::get, Router};
use key_package::KeyPackage;
mod key_package;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/packages", get(get_key_packages).post(create_key_package));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn create_key_package(KeyPackage(package): KeyPackage) -> Result<(), ()> {
    print!("Received key package {:?}", package);
    Ok(())
}

async fn get_key_packages() -> Result<(), ()> {
    Ok(())
}
