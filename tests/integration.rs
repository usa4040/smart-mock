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

fn app_from(spec: &str) -> axum::Router {
    let routes = schema::parse_openapi(spec).unwrap();
    build_router(routes)
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
    assert_eq!(
        res.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*")
    );
    let body = body_json(res).await;
    let arr = body.as_array().expect("expected array");
    assert_eq!(arr.len(), 3);
    let first = &arr[0];
    assert!(first.get("user_email").and_then(|v| v.as_str()).unwrap().contains('@'));
    assert!(first.get("id").is_some());
}

#[tokio::test]
async fn post_then_get_reflects_state() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
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
    assert!(created.get("created_at").and_then(|v| v.as_str()).is_some());
    assert_eq!(created["user_email"], json!("alice@example.com"));

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users?full_name=nobody")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = body_json(list).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr.iter().any(|u| u.get("id") == Some(&id)));
    assert!(arr
        .iter()
        .any(|u| u.get("user_email") == Some(&json!("alice@example.com"))));
}

#[tokio::test]
async fn missing_item_is_404_and_put_replaces() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
    let app = build_router(routes);

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/99999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

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
    assert_eq!(updated["user_email"], json!("upd@example.com"));
    assert!(updated.get("created_at").is_some());

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/users/{id_str}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "full_name": "Patched" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let patched = body_json(patch).await;
    assert_eq!(patched["full_name"], json!("Patched"));
    assert_eq!(patched["user_email"], json!("upd@example.com"));

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
async fn unknown_path_and_bad_body_are_400() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
    let app = build_router(routes);
    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    let bad = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "full_name": "Alice" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reset_clears_records() {
    let routes = schema::load_routes(std::path::Path::new("examples/openapi.yaml")).unwrap();
    let app = build_router(routes);
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "user_email": "alice@example.com", "full_name": "Alice" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = body_json(create).await;
    let id = created["id"].to_string();

    let reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/__smart_mock/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::NO_CONTENT);

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrapped_list_and_servers_path_are_not_recorded() {
    let spec = r#"
openapi: "3.0.3"
info: { title: t, version: "1" }
servers:
  - url: https://api.example.com/v1
paths:
  /search:
    post:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                q: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  items:
                    type: array
                    items: { type: object }
              example:
                items: [{ "q": "kept-example" }]
  /wrapped:
    get:
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  items:
                    type: array
                    items:
                      type: object
                      properties:
                        id: { type: integer }
    post:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                id: { type: integer }
                name: { type: string }
      responses:
        "201":
          description: created
          content:
            application/json:
              schema:
                type: object
  /wrapped/{id}:
    get:
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: integer }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
"#;
    let app = app_from(spec);
    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/search")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "q": "x" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::BAD_REQUEST);

    let example = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "q": "x" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(example.status(), StatusCode::OK);
    let body = body_json(example).await;
    assert_eq!(body["items"][0]["q"], json!("kept-example"));

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/wrapped")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "nope" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let list = app
        .oneshot(
            Request::builder()
                .uri("/wrapped")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let listed = body_json(list).await;
    assert!(listed.get("items").is_some());
    assert!(listed.get("name").is_none());
}

#[tokio::test]
async fn security_requires_presence_not_value() {
    let spec = r#"
openapi: "3.0.3"
info: { title: t, version: "1" }
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
paths:
  /notes:
    get:
      security:
        - bearerAuth: []
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: array
                items:
                  type: object
                  properties:
                    id: { type: string }
  /notes/{id}:
    get:
      security:
        - {}
        - bearerAuth: []
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
"#;
    let app = app_from(spec);
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/notes")
                .header("authorization", "Bearer not-checked")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    let optional = app
        .oneshot(
            Request::builder()
                .uri("/notes/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(optional.status(), StatusCode::NOT_FOUND);
}
