use std::collections::HashMap;

use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config_url: String;
    let self_ip: String;
    if args.len() != 3 {
        println!("Usage: {} <config_url> <self_ip>", args[0]);
        return;
    } else {
        config_url = args[1].clone();
        self_ip = args[2].clone();
    }

    let config_client = reqwest::Client::new();
    let map: HashMap<&str, &str> = [("port", "13227"), ("ip", &self_ip)].into_iter().collect();
    let res = config_client
        .post(config_url + "/set_ip/messages/0")
        .json(&map)
        .send()
        .await
        .unwrap();

    if !res.status().is_success() {
        println!("Error notifying config: {:?}", res);
        return;
    }

    let app = Router::new().route("/", get(|| async { "Not implemented" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:13227")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
