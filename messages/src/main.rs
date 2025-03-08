use axum::{
    routing::get,
    Router,
};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Not implemented" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:13227").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
