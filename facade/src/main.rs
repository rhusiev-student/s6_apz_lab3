use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use axum::extract::State;
use axum::response::IntoResponse;
use logging::logging_service_client::LoggingServiceClient;
use logging::{GetLogsRequest, Log};
use reqwest::StatusCode;
use tonic::transport::Endpoint;
use uuid::Uuid;

pub mod logging {
    tonic::include_proto!("logging");
}

use axum::{
    extract::Query,
    routing::{get, post},
    Router,
};

#[derive(Clone)]
struct Client {
    logging: Arc<Mutex<LoggingServiceClient<tonic::transport::Channel>>>,
    message: Arc<Mutex<reqwest::Client>>,
    message_url: String,
}

async fn create_grpc_client(
) -> Result<LoggingServiceClient<tonic::transport::Channel>, tonic::transport::Error> {
    let channel = Endpoint::from_static("http://localhost:13228")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .connect()
        .await?;

    Ok(LoggingServiceClient::new(channel))
}

use std::future::Future;
use tokio::time::sleep;

async fn retry<T, E: std::fmt::Debug, Fut, F, R, RFut>(
    mut operation: F,
    max_retries: u32,
    delay: Duration,
    mut refresh: Option<R>,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    R: FnMut(&E) -> RFut,
    RFut: Future<Output = ()>,
{
    let mut attempts = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                println!("Error on attempt {}: {:?}", attempts + 1, err);
                if attempts >= max_retries {
                    return Err(err);
                }
                if let Some(ref mut refresh_fn) = refresh {
                    refresh_fn(&err).await;
                }
                attempts += 1;
                sleep(delay).await;
            }
        }
    }
}

async fn add_log(
    State(client): State<Client>,
    Query(log): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let uuid = Uuid::new_v4().to_string();
    let message = match log.get("message") {
        Some(msg) => msg.to_string(),
        None => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let request = tonic::Request::new(Log { uuid, message });

    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 500;

    let logging_arc = client.logging.clone();

    let status = retry(
        || async {
            let mut grpc_client = logging_arc.lock().await;
            grpc_client.add_log(request.get_ref().clone()).await
        },
        MAX_RETRIES,
        Duration::from_millis(RETRY_DELAY_MS),
        Some(|err: &tonic::Status| {
            let error_code = err.code();
            let logging_arc = logging_arc.clone();
            async move {
                if error_code == tonic::Code::Unavailable {
                    if let Ok(new_client) = create_grpc_client().await {
                        let mut grpc_client = logging_arc.lock().await;
                        *grpc_client = new_client;
                    }
                }
            }
        }),
    )
    .await;

    match status {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_logs(State(client): State<Client>) -> impl IntoResponse {
    let message = match client
        .message
        .lock()
        .await
        .get(&client.message_url)
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(msg) => msg,
            Err(err) => {
                println!("Error while getting a message request: {}", err);
                return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
            }
        },
        Err(err) => {
            println!("Error while creating a message request: {}", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };

    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 500;

    let logging_arc = client.logging.clone();
    let logs = tonic::Request::new(GetLogsRequest {});

    let status = retry(
        || async {
            let mut grpc_client = logging_arc.lock().await;
            grpc_client.get_logs(logs.get_ref().clone()).await
        },
        MAX_RETRIES,
        Duration::from_millis(RETRY_DELAY_MS),
        Some(|err: &tonic::Status| {
            let error_code = err.code();
            let logging_arc = logging_arc.clone();
            async move {
                if error_code == tonic::Code::Unavailable {
                    if let Ok(new_client) = create_grpc_client().await {
                        let mut grpc_client = logging_arc.lock().await;
                        *grpc_client = new_client;
                    }
                }
            }
        }),
    )
    .await;

    match status {
        Ok(response) => {
            return (
                StatusCode::OK,
                (message + "\n" + &response.into_inner().logs_string),
            )
                .into_response()
        }
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.message().to_string()).into_response();
        }
    }
}

#[tokio::main]
async fn main() {
    let grpc_client = create_grpc_client()
        .await
        .expect("Failed to connect to logging service");

    let client = Client {
        logging: Arc::new(Mutex::new(grpc_client)),
        message: Arc::new(Mutex::new(reqwest::Client::new())),
        message_url: "http://localhost:13227".to_string(),
    };

    let app = Router::new()
        .route("/", get(get_logs))
        .with_state(client.clone())
        .route("/", post(add_log))
        .with_state(client);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:13226")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
