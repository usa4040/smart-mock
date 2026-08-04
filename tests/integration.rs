use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use smart_mock::schema;
use smart_mock::server::build_router;
use tower::ServiceExt;

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(json!(null))
}

#[tokio::test]
async fn openapi_get_users_returns_array() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
    let app = build_router(routes);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().expect("expected array");
    assert!(!arr.is_empty());
    let first = &arr[0];
    assert!(first.get("user_email").and_then(|v| v.as_str()).unwrap().contains('@'));
    assert!(first.get("id").is_some());
}

#[tokio::test]
async fn post_then_get_reflects_state() {
    let routes = schema::load_routes(std::path::Path::new("examples/schema.json")).unwrap();
    let app = build_router(routes);

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "user_email": "alice@example.com",
                        "full_name": "Alice"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created = body_json(create).await;
    let id = created.get("id").cloned().unwrap();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = body_json(list).await;
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|u| u.get("id") == Some(&id)));
    assert!(arr
        .iter()
        .any(|u| u.get("user_email") == Some(&json!("alice@example.com"))));
}

#[tokio::test]
async fn put_and_delete_work() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
    let app = build_router(routes);

    // Seed via GET
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(list).await;
    let id = body.as_array().unwrap()[0]["id"].clone();
    let id_str = match &id {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/users/{id_str}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "full_name": "Updated Name", "user_email": "upd@example.com" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
    let updated = body_json(put).await;
    assert_eq!(updated["full_name"], json!("Updated Name"));

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/users/{id_str}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{id_str}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn typescript_schema_loads() {
    let routes = schema::load_routes(std::path::Path::new("examples/users.ts")).unwrap();
    assert!(routes.iter().any(|r| r.path == "/api/users"));
    let app = build_router(routes);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
