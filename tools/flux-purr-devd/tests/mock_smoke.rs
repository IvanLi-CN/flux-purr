use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use flux_purr_devd::{AppState, app};
use serde_json::Value;
use tower::ServiceExt;

async fn request_json(router: &Router, method: Method, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn mock_http_smoke_covers_read_only_control_contract() {
    let router = app(AppState::test());

    let (status, health) = request_json(&router, Method::GET, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["name"], "flux-purr-devd");

    let (status, devices) = request_json(&router, Method::GET, "/api/v1/devices").await;
    assert_eq!(status, StatusCode::OK);
    let device = devices["devices"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == "mock-fp-lab-01"))
        .expect("seeded mock device is discoverable");
    assert_eq!(device["transport"], "mock");

    let (status, lease) = request_json(
        &router,
        Method::POST,
        "/api/v1/devices/mock-fp-lab-01/leases",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let lease_id = lease["leaseId"].as_str().expect("lease id");
    let suffix = format!("?lease_id={lease_id}");

    let (status, identity) = request_json(
        &router,
        Method::GET,
        &format!("/api/v1/devices/mock-fp-lab-01/identity{suffix}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(identity["deviceId"], "mock-fp-lab-01");

    let (status, runtime) = request_json(
        &router,
        Method::GET,
        &format!("/api/v1/devices/mock-fp-lab-01/status{suffix}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(runtime["heaterEnabled"], true);
}
