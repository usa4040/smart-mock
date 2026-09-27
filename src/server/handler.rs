use crate::generate::check::{check_body, query_matches, CheckMode};
use crate::generate::{assign_missing_id, extract_id, fill_missing, force_path_id, generate, generate_item};
use crate::schema::types::{
    AuthRequirement, AuthScheme, ParamIn, RouteInfo, SchemaNode, RESET_PATH,
};
use crate::server::state::AppState;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub async fn fallback_handler(State(state): State<AppState>, req: Request) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let path = uri.path().to_string();

    if method == Method::OPTIONS {
        return finish(StatusCode::NO_CONTENT.into_response());
    }

    if method == Method::POST && path == RESET_PATH {
        state.store.write().await.clear();
        return finish(StatusCode::NO_CONTENT.into_response());
    }

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => Bytes::new(),
    };
    let query = query_map(uri.query().unwrap_or(""));

    let Some((route, params)) = find_route(&state.routes, &method, &path) else {
        return error(
            StatusCode::BAD_REQUEST,
            format!("no matching operation for {} {path}", method.as_str()),
        );
    };

    if let Err(message) = require_auth(&route.auth, &headers, &query) {
        return error(StatusCode::UNAUTHORIZED, message);
    }
    if let Err(message) = require_params(route, &headers, &query, &params) {
        return error(StatusCode::BAD_REQUEST, message);
    }

    if !route.recorded {
        return finish(stateless_response(route));
    }

    let response = match method {
        Method::GET if is_item(route) => handle_get_item(&state, route, &params).await,
        Method::GET => handle_get_collection(&state, route).await,
        Method::POST => handle_post(&state, route, &body).await,
        Method::PUT => handle_put(&state, route, &params, &body).await,
        Method::PATCH => handle_patch(&state, route, &params, &body).await,
        Method::DELETE => handle_delete(&state, route, &params).await,
        _ => error(
            StatusCode::BAD_REQUEST,
            format!("no matching operation for {} {path}", method.as_str()),
        ),
    };
    finish(response)
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
        if let Some(params) = crate::schema::types::match_path(&route.path, path) {
            return Some((route, params));
        }
    }
    None
}

fn is_item(route: &RouteInfo) -> bool {
    crate::schema::types::is_item_path(&route.path)
}

fn stateless_response(route: &RouteInfo) -> Response {
    if route.status == 204 {
        return StatusCode::NO_CONTENT.into_response();
    }
    let body = route
        .example
        .clone()
        .unwrap_or_else(|| generate(&route.response_schema));
    let status = StatusCode::from_u16(route.status).unwrap_or(StatusCode::OK);
    (status, Json(body)).into_response()
}

async fn handle_get_collection(state: &AppState, route: &RouteInfo) -> Response {
    let collection = route.collection_key.clone();
    {
        let store = state.store.read().await;
        if let Some(items) = store.get(&collection) {
            return json_status(route.status, Value::Array(items.clone()));
        }
    }
    let seeded = initial_records(route);
    let mut store = state.store.write().await;
    let items = store.entry(collection).or_insert_with(|| seeded.clone());
    json_status(route.status, Value::Array(items.clone()))
}

async fn handle_get_item(
    state: &AppState,
    route: &RouteInfo,
    params: &indexmap::IndexMap<String, String>,
) -> Response {
    let Some(id) = params.get("id") else {
        return error(StatusCode::BAD_REQUEST, "missing id".into());
    };
    let store = state.store.read().await;
    let Some(items) = store.get(&route.collection_key) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    match items.iter().find(|value| extract_id(value).as_deref() == Some(id)) {
        Some(item) => json_status(route.status, item.clone()),
        None => error(StatusCode::NOT_FOUND, format!("item not found: {id}")),
    }
}

async fn handle_post(state: &AppState, route: &RouteInfo, body: &Bytes) -> Response {
    let schema = write_schema(route);
    let mut value = match parse_object(body) {
        Ok(value) => value,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(message) = check_body(schema, &value, CheckMode::Write) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    fill_missing(schema, &mut value);
    let collection = route.collection_key.clone();
    let mut store = state.store.write().await;
    let items = store.entry(collection).or_default();
    assign_missing_id(&mut value, route.id_kind, items);
    items.push(value.clone());
    json_status(route.status, value)
}

async fn handle_put(
    state: &AppState,
    route: &RouteInfo,
    params: &indexmap::IndexMap<String, String>,
    body: &Bytes,
) -> Response {
    let Some(id) = params.get("id").cloned() else {
        return error(StatusCode::BAD_REQUEST, "missing id".into());
    };
    let schema = write_schema(route);
    let mut value = match parse_object(body) {
        Ok(value) => value,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(message) = check_body(schema, &value, CheckMode::Write) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    force_path_id(&mut value, &id, route.id_kind);
    fill_missing(schema, &mut value);
    let mut store = state.store.write().await;
    let Some(items) = store.get_mut(&route.collection_key) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    let Some(pos) = items.iter().position(|item| extract_id(item).as_deref() == Some(id.as_str())) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    items[pos] = value.clone();
    json_status(route.status, value)
}

async fn handle_patch(
    state: &AppState,
    route: &RouteInfo,
    params: &indexmap::IndexMap<String, String>,
    body: &Bytes,
) -> Response {
    let Some(id) = params.get("id").cloned() else {
        return error(StatusCode::BAD_REQUEST, "missing id".into());
    };
    let schema = write_schema(route);
    let incoming = match parse_object(body) {
        Ok(value) => value,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(message) = check_body(schema, &incoming, CheckMode::Patch) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    let mut store = state.store.write().await;
    let Some(items) = store.get_mut(&route.collection_key) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    let Some(pos) = items.iter().position(|item| extract_id(item).as_deref() == Some(id.as_str())) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    let mut merged = items[pos].as_object().cloned().unwrap_or_else(Map::new);
    if let Some(fields) = incoming.as_object() {
        for (key, value) in fields {
            merged.insert(key.clone(), value.clone());
        }
    }
    let mut value = Value::Object(merged);
    force_path_id(&mut value, &id, route.id_kind);
    items[pos] = value.clone();
    json_status(route.status, value)
}

async fn handle_delete(
    state: &AppState,
    route: &RouteInfo,
    params: &indexmap::IndexMap<String, String>,
) -> Response {
    let Some(id) = params.get("id") else {
        return error(StatusCode::BAD_REQUEST, "missing id".into());
    };
    let mut store = state.store.write().await;
    let Some(items) = store.get_mut(&route.collection_key) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    let Some(pos) = items.iter().position(|item| extract_id(item).as_deref() == Some(id.as_str())) else {
        return error(StatusCode::NOT_FOUND, format!("item not found: {id}"));
    };
    let removed = items.remove(pos);
    if route.status == 204 {
        StatusCode::NO_CONTENT.into_response()
    } else {
        json_status(route.status, removed)
    }
}

fn write_schema(route: &RouteInfo) -> &SchemaNode {
    route
        .request_schema
        .as_ref()
        .unwrap_or_else(|| route.response_schema.item_schema())
}

fn initial_records(route: &RouteInfo) -> Vec<Value> {
    let mut records = match &route.example {
        Some(Value::Array(values)) if values.iter().all(Value::is_object) && !values.is_empty() => {
            values.clone()
        }
        Some(Value::Object(_)) => vec![route.example.clone().unwrap()],
        _ => {
            let mut generated: Vec<Value> = (0..3)
                .map(|_| generate_item(&route.response_schema))
                .collect();
            for record in &mut generated {
                if let Some(map) = record.as_object_mut() {
                    map.remove("id");
                }
            }
            generated
        }
    };
    let mut assigned = Vec::new();
    for record in &mut records {
        assign_missing_id(record, route.id_kind, &assigned);
        assigned.push(record.clone());
    }
    records
}

fn parse_object(body: &Bytes) -> Result<Value, String> {
    if body.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value = serde_json::from_slice(body).map_err(|err| format!("invalid JSON: {err}"))?;
    if !value.is_object() {
        return Err("body expected object".into());
    }
    Ok(value)
}

fn require_auth(
    auth: &AuthRequirement,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
) -> Result<(), String> {
    let AuthRequirement::Required(groups) = auth else {
        return Ok(());
    };
    let ok = groups.iter().any(|group| {
        group.iter().all(|scheme| scheme_present(scheme, headers, query))
    });
    if ok {
        Ok(())
    } else {
        Err("missing credentials".into())
    }
}

fn scheme_present(scheme: &AuthScheme, headers: &HeaderMap, query: &HashMap<String, String>) -> bool {
    match scheme {
        AuthScheme::AuthorizationHeader => header_value(headers, "authorization").is_some(),
        AuthScheme::ApiKeyHeader(name) => header_value(headers, name).is_some(),
        AuthScheme::ApiKeyQuery(name) => query.get(name).is_some_and(|value| !value.is_empty()),
        AuthScheme::ApiKeyCookie(name) => cookie_value(headers, name).is_some(),
    }
}

fn require_params(
    route: &RouteInfo,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    path_params: &indexmap::IndexMap<String, String>,
) -> Result<(), String> {
    for param in &route.params {
        let raw = match param.location {
            ParamIn::Query => query.get(&param.name).cloned(),
            ParamIn::Header => header_value(headers, &param.name),
            ParamIn::Path => path_params.get(&param.name).cloned(),
            ParamIn::Cookie => cookie_value(headers, &param.name),
        };
        match raw {
            None if param.required => {
                return Err(format!("missing required {} {}", location_name(param.location), param.name));
            }
            None => {}
            Some(value) if !query_matches(&param.schema, &value) => {
                return Err(format!("parameter {} has the wrong type", param.name));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn location_name(location: ParamIn) -> &'static str {
    match location {
        ParamIn::Query => "query",
        ParamIn::Header => "header",
        ParamIn::Path => "path",
        ParamIn::Cookie => "cookie",
    }
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = header_value(headers, "cookie")?;
    for part in raw.split(';') {
        let mut pair = part.trim().splitn(2, '=');
        if pair.next() == Some(name) {
            return pair.next().filter(|value| !value.is_empty()).map(str::to_string);
        }
    }
    None
}

fn query_map(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if query.is_empty() {
        return map;
    }
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let name = decode(parts.next().unwrap_or(""));
        let value = decode(parts.next().unwrap_or(""));
        if !name.is_empty() {
            map.insert(name, value);
        }
    }
    map
}

fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let hex = &input[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn json_status(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    if code == StatusCode::NO_CONTENT {
        return StatusCode::NO_CONTENT.into_response();
    }
    (code, Json(body)).into_response()
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn finish(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        header::HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        header::HeaderValue::from_static("*"),
    );
    response
}
