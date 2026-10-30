//! A tiny API to try `stormix run` against, used by CI as well:
//! `cargo run -p stormix-engine --example demo_api`, then
//! `stormix run examples/demo-api.stormix.yaml`.

use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

const ADDR: &str = "127.0.0.1:3999";

async fn login(Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    if body["password"] == "secret" {
        let user = body["user"].as_str().unwrap_or_default();
        (
            StatusCode::OK,
            Json(json!({ "token": format!("tok-{user}") })),
        )
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "wrong password" })),
        )
    }
}

async fn me(headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer tok-"));
    match token {
        Some(user) => (
            StatusCode::OK,
            Json(json!({ "user": user, "roles": ["tester"] })),
        ),
        None => (StatusCode::UNAUTHORIZED, Json(json!({}))),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/login", post(login))
        .route("/me", get(me));
    let listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
    println!("demo API listening on http://{ADDR}");
    axum::serve(listener, app).await.unwrap();
}
