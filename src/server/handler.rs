use crate::generate::{ensure_id, extract_id, generate, generate_item, generate_items};
use crate::schema::types::{is_item_path, match_path, RouteInfo};
use crate::server::state::AppState;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

pub async fn fallback_handler(State(state): State<AppState>, req: Request) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let body = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => Bytes::new(),
    };

    let matched = find_route(&state.routes, &method, &path);
    let Some((route, params)) = matched else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not found", "path": path, "method": method.as_str() })),
        )
            .into_response();
    };

    let collection = route.collection_key.clone();
    let item_mode = is_item_path(&route.path) || params.contains_key("id");

    match method {
        Method::GET if item_mode => handle_get_item(&state, &collection, &params, route).await,
        Method::GET => handle_get_collection(&state, &collection, route).await,
        Method::POST => handle_post(&state, &collection, &body, route).await,
        Method::PUT | Method::PATCH => {
            handle_put(&state, &collection, &params, &body, route).await
        }
        Method::DELETE => handle_delete(&state, &collection, &params).await,
        _ => (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({ "error": "method not allowed" })),
        )
            .into_response(),
    }
}

fn find_route<'a>(
    routes: &'a [RouteInfo],
    method: &Method,
    path: &str,
) -> Option<(&'a RouteInfo, indexmap::IndexMap<String, String>)> {
    for route in routes {
        if &route.method != method {
            continue;
        }
        if let Some(params) = match_path(&route.path, path) {
            return Some((route, params));
        }
    }
    None
}

async fn handle_get_collection(state: &AppState, collection: &str, route: &RouteInfo) -> Response {
    {
        let store = state.store.read().await;
        if let Some(items) = store.get(collection) {
            if !items.is_empty() {
                return (StatusCode::OK, Json(Value::Array(items.clone()))).into_response();
            }
        }
    }

    let seeded = generate_items(&route.response_schema, 3);
    {
        let mut store = state.store.write().await;
        store.insert(collection.to_string(), seeded.clone());
    }
    (StatusCode::OK, Json(Value::Array(seeded))).into_response()
}

async fn handle_get_item(
    state: &AppState,
    collection: &str,
    params: &indexmap::IndexMap<String, String>,
    route: &RouteInfo,
) -> Response {
    let id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing id" })),
            )
                .into_response();
        }
    };

    {
        let store = state.store.read().await;
        if let Some(items) = store.get(collection) {
            if let Some(item) = items
                .iter()
                .find(|v| extract_id(v).as_deref() == Some(id.as_str()))
            {
                return (StatusCode::OK, Json(item.clone())).into_response();
            }
        }
    }

    {
        let store = state.store.read().await;
        if !store.contains_key(collection) {
            drop(store);
            let mut item = generate_item(&route.response_schema);
            if let Value::Object(ref mut map) = item {
                if let Ok(n) = id.parse::<i64>() {
                    map.insert("id".into(), json!(n));
                } else {
                    map.insert("id".into(), json!(id));
                }
            }
            let mut store = state.store.write().await;
            store.insert(collection.to_string(), vec![item.clone()]);
            return (StatusCode::OK, Json(item)).into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "item not found", "id": id })),
    )
        .into_response()
}

async fn handle_post(
    state: &AppState,
    collection: &str,
    body: &Bytes,
    route: &RouteInfo,
) -> Response {
    let mut value: Value = if body.is_empty() {
        generate_item(&route.response_schema)
    } else {
        match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("invalid JSON: {e}") })),
                )
                    .into_response();
            }
        }
    };

    let next = state.next_id(collection).await;
    value = ensure_id(value, next);

    {
        let mut store = state.store.write().await;
        store
            .entry(collection.to_string())
            .or_default()
            .push(value.clone());
    }

    let status = StatusCode::from_u16(route.status).unwrap_or(StatusCode::CREATED);
    (status, Json(value)).into_response()
}

async fn handle_put(
    state: &AppState,
    collection: &str,
    params: &indexmap::IndexMap<String, String>,
    body: &Bytes,
    route: &RouteInfo,
) -> Response {
    let id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing id" })),
            )
                .into_response();
        }
    };

    let mut incoming: Value = if body.is_empty() {
        generate(route.response_schema.item_schema())
    } else {
        match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("invalid JSON: {e}") })),
                )
                    .into_response();
            }
        }
    };

    if let Value::Object(ref mut map) = incoming {
        if let Ok(n) = id.parse::<i64>() {
            map.insert("id".into(), json!(n));
        } else {
            map.insert("id".into(), json!(id.clone()));
        }
    }

    let mut store = state.store.write().await;
    let items = store.entry(collection.to_string()).or_default();
    if let Some(pos) = items
        .iter()
        .position(|v| extract_id(v).as_deref() == Some(id.as_str()))
    {
        if let (Value::Object(existing), Value::Object(new_map)) =
            (items[pos].clone(), incoming.clone())
        {
            let mut merged = existing;
            for (k, v) in new_map {
                merged.insert(k, v);
            }
            items[pos] = Value::Object(merged);
        } else {
            items[pos] = incoming.clone();
        }
        let status = StatusCode::from_u16(route.status).unwrap_or(StatusCode::OK);
        return (status, Json(items[pos].clone())).into_response();
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "item not found", "id": id })),
    )
        .into_response()
}

async fn handle_delete(
    state: &AppState,
    collection: &str,
    params: &indexmap::IndexMap<String, String>,
) -> Response {
    let id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing id" })),
            )
                .into_response();
        }
    };

    let mut store = state.store.write().await;
    let items = store.entry(collection.to_string()).or_default();
    let before = items.len();
    items.retain(|v| extract_id(v).as_deref() != Some(id.as_str()));
    if items.len() < before {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "item not found", "id": id })),
        )
            .into_response()
    }
}
