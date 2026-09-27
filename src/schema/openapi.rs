use crate::error::{AppError, Result};
use crate::schema::types::{
    collection_key_from_path, AuthRequirement, AuthScheme, IdKind, ParamIn, RequestParam, RouteInfo,
    SchemaNode,
};
use axum::http::Method;
use indexmap::IndexMap;
use openapiv3::{
    OpenAPI, ReferenceOr, Schema, SchemaKind as OasSchemaKind, StatusCode, Type as OasType,
    VariantOrUnknownOrEmpty,
};
use serde_json::Value;

/// Parse OpenAPI v3 document (JSON or YAML) into route infos.
pub fn parse(content: &str) -> Result<Vec<RouteInfo>> {
    let api: OpenAPI = if content.trim_start().starts_with('{') {
        serde_json::from_str(content).map_err(|e| AppError::Parse(e.to_string()))?
    } else {
        serde_yaml::from_str(content).map_err(|e| AppError::Parse(e.to_string()))?
    };

    let mut routes = Vec::new();

    for (path, path_item_ref) in &api.paths.paths {
        let path_item = match path_item_ref {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => continue,
        };

        let ops: Vec<(Method, &openapiv3::Operation)> = [
            (Method::GET, path_item.get.as_ref()),
            (Method::POST, path_item.post.as_ref()),
            (Method::PUT, path_item.put.as_ref()),
            (Method::DELETE, path_item.delete.as_ref()),
            (Method::PATCH, path_item.patch.as_ref()),
        ]
        .into_iter()
        .filter_map(|(m, op)| op.map(|o| (m, o)))
        .collect();

        for (method, operation) in ops {
            let (status, schema_node) = extract_success_schema(operation, &api);
            routes.push(RouteInfo {
                method,
                path: path.clone(),
                collection_key: collection_key_from_path(path),
                recorded: false,
                response_schema: schema_node,
                request_schema: extract_request_schema(operation, &api),
                status,
                example: extract_example(operation, &api),
                auth: operation_auth(operation, &api),
                params: operation_params(path_item, operation, &api),
                id_kind: IdKind::String,
            });
        }
    }

    mark_recorded_collections(&mut routes);
    Ok(routes)
}

fn additional_properties_allowed(value: &Option<openapiv3::AdditionalProperties>) -> bool {
    !matches!(value, Some(openapiv3::AdditionalProperties::Any(false)))
}

fn extract_request_schema(operation: &openapiv3::Operation, api: &OpenAPI) -> Option<SchemaNode> {
    let body_ref = operation.request_body.as_ref()?;
    let body = match body_ref {
        ReferenceOr::Item(body) => body,
        ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/requestBodies/")?;
            let components = api.components.as_ref()?;
            match components.request_bodies.get(name)? {
                ReferenceOr::Item(body) => body,
                ReferenceOr::Reference { .. } => return None,
            }
        }
    };
    let media = body.content.get("application/json")?;
    let schema_ref = media.schema.as_ref()?;
    Some(resolve_schema(schema_ref, api, 0))
}

fn extract_example(operation: &openapiv3::Operation, api: &OpenAPI) -> Option<Value> {
    let resp_ref = success_response(operation)?;
    let response = resolve_response(resp_ref, api)?;
    let media = response.content.get("application/json")?;
    if let Some(example) = &media.example {
        return Some(example.clone());
    }
    for example_ref in media.examples.values() {
        let example = match example_ref {
            ReferenceOr::Item(example) => example,
            ReferenceOr::Reference { reference } => {
                let Some(name) = reference.strip_prefix("#/components/examples/") else {
                    continue;
                };
                let Some(components) = api.components.as_ref() else {
                    continue;
                };
                let Some(resolved) = components.examples.get(name) else {
                    continue;
                };
                match resolved {
                    ReferenceOr::Item(example) => example,
                    ReferenceOr::Reference { .. } => continue,
                }
            }
        };
        if let Some(value) = &example.value {
            return Some(value.clone());
        }
    }
    None
}

fn success_response(operation: &openapiv3::Operation) -> Option<&ReferenceOr<openapiv3::Response>> {
    let responses = &operation.responses.responses;
    let preferred = [
        StatusCode::Code(200),
        StatusCode::Code(201),
        StatusCode::Code(204),
    ];
    for code in &preferred {
        if responses.contains_key(code) {
            return responses.get(code);
        }
    }
    responses.iter().find_map(|(code, resp)| match code {
        StatusCode::Code(c) if (200..300).contains(c) => Some(resp),
        _ => None,
    })
}

fn resolve_response<'a>(
    resp_ref: &'a ReferenceOr<openapiv3::Response>,
    api: &'a OpenAPI,
) -> Option<&'a openapiv3::Response> {
    match resp_ref {
        ReferenceOr::Item(response) => Some(response),
        ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/responses/")?;
            match api.components.as_ref()?.responses.get(name)? {
                ReferenceOr::Item(response) => Some(response),
                ReferenceOr::Reference { .. } => None,
            }
        }
    }
}

fn operation_auth(operation: &openapiv3::Operation, api: &OpenAPI) -> AuthRequirement {
    let Some(requirements) = operation.security.as_ref().or(api.security.as_ref()) else {
        return AuthRequirement::None;
    };
    if requirements.is_empty() || requirements.iter().any(|req| req.is_empty()) {
        return AuthRequirement::None;
    }
    let groups = requirements
        .iter()
        .map(|req| {
            req.keys()
                .map(|name| scheme_to_auth(name, api))
                .collect()
        })
        .collect();
    AuthRequirement::Required(groups)
}

fn scheme_to_auth(name: &str, api: &OpenAPI) -> AuthScheme {
    let Some(components) = api.components.as_ref() else {
        return AuthScheme::AuthorizationHeader;
    };
    let Some(scheme_ref) = components.security_schemes.get(name) else {
        return AuthScheme::AuthorizationHeader;
    };
    let scheme = match scheme_ref {
        ReferenceOr::Item(scheme) => scheme,
        ReferenceOr::Reference { .. } => return AuthScheme::AuthorizationHeader,
    };
    match scheme {
        openapiv3::SecurityScheme::APIKey { location, name, .. } => match location {
            openapiv3::APIKeyLocation::Query => AuthScheme::ApiKeyQuery(name.clone()),
            openapiv3::APIKeyLocation::Header => AuthScheme::ApiKeyHeader(name.clone()),
            openapiv3::APIKeyLocation::Cookie => AuthScheme::ApiKeyCookie(name.clone()),
        },
        _ => AuthScheme::AuthorizationHeader,
    }
}

fn operation_params(
    path_item: &openapiv3::PathItem,
    operation: &openapiv3::Operation,
    api: &OpenAPI,
) -> Vec<RequestParam> {
    let mut params: Vec<RequestParam> = Vec::new();
    for parameter in path_item.parameters.iter().chain(operation.parameters.iter()) {
        if let Some(param) = convert_parameter(parameter, api) {
            params.retain(|existing| {
                !(existing.name == param.name && existing.location == param.location)
            });
            params.push(param);
        }
    }
    params
}

fn convert_parameter(
    parameter: &ReferenceOr<openapiv3::Parameter>,
    api: &OpenAPI,
) -> Option<RequestParam> {
    let parameter = match parameter {
        ReferenceOr::Item(parameter) => parameter,
        ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/parameters/")?;
            match api.components.as_ref()?.parameters.get(name)? {
                ReferenceOr::Item(parameter) => parameter,
                ReferenceOr::Reference { .. } => return None,
            }
        }
    };
    let (location, data) = match parameter {
        openapiv3::Parameter::Query { parameter_data, .. } => (ParamIn::Query, parameter_data),
        openapiv3::Parameter::Header { parameter_data, .. } => (ParamIn::Header, parameter_data),
        openapiv3::Parameter::Path { parameter_data, .. } => (ParamIn::Path, parameter_data),
        openapiv3::Parameter::Cookie { parameter_data, .. } => (ParamIn::Cookie, parameter_data),
    };
    let schema = match &data.format {
        openapiv3::ParameterSchemaOrContent::Schema(schema) => resolve_schema(schema, api, 0),
        openapiv3::ParameterSchemaOrContent::Content(_) => SchemaNode::Any,
    };
    Some(RequestParam {
        location,
        name: data.name.clone(),
        required: data.required,
        schema,
    })
}

fn mark_recorded_collections(routes: &mut [RouteInfo]) {
    let keys: Vec<String> = routes
        .iter()
        .map(|route| route.collection_key.clone())
        .collect();
    let mut seen = Vec::new();
    for key in keys {
        if seen.contains(&key) {
            continue;
        }
        seen.push(key.clone());
        let item_brace = format!("{key}/{{id}}");
        let item_colon = format!("{key}/:id");
        let has_list = routes.iter().any(|route| route.path == key);
        let has_item = routes
            .iter()
            .any(|route| route.path == item_brace || route.path == item_colon);
        let list_is_array = routes.iter().any(|route| {
            route.path == key
                && route.method == Method::GET
                && matches!(route.response_schema, SchemaNode::Array { .. })
        });
        if !(has_list && has_item && list_is_array) {
            continue;
        }
        let id_kind = routes
            .iter()
            .find(|route| route.path == item_brace || route.path == item_colon)
            .and_then(|route| {
                route.params.iter().find(|param| {
                    param.location == ParamIn::Path && param.name == "id"
                })
            })
            .map(|param| match &param.schema {
                SchemaNode::Integer { .. } | SchemaNode::Number => IdKind::Integer,
                _ => IdKind::String,
            })
            .unwrap_or(IdKind::String);

        for route in routes.iter_mut() {
            let list_write = route.path == key
                && (route.method == Method::GET || route.method == Method::POST);
            let item_write = (route.path == item_brace || route.path == item_colon)
                && matches!(
                    route.method,
                    Method::GET | Method::PUT | Method::PATCH | Method::DELETE
                );
            if list_write || item_write {
                route.recorded = true;
                route.id_kind = id_kind;
            }
        }
    }
}

fn extract_success_schema(operation: &openapiv3::Operation, api: &OpenAPI) -> (u16, SchemaNode) {
    let responses = &operation.responses.responses;

    // Prefer 200, then 201, then any 2xx
    let preferred = [
        StatusCode::Code(200),
        StatusCode::Code(201),
        StatusCode::Code(204),
    ];

    for code in &preferred {
        if let Some(resp_ref) = responses.get(code) {
            if let Some(node) = response_to_schema(resp_ref, api) {
                let status = match code {
                    StatusCode::Code(c) => *c,
                    _ => 200,
                };
                return (status, node);
            }
            if matches!(code, StatusCode::Code(204)) {
                return (204, SchemaNode::Any);
            }
        }
    }

    // Any other 2xx
    for (code, resp_ref) in responses {
        if let StatusCode::Code(c) = code {
            if (200..300).contains(c) {
                if let Some(node) = response_to_schema(resp_ref, api) {
                    return (*c, node);
                }
            }
        }
    }

    (200, SchemaNode::empty_object())
}

fn response_to_schema(resp_ref: &ReferenceOr<openapiv3::Response>, api: &OpenAPI) -> Option<SchemaNode> {
    let response = match resp_ref {
        ReferenceOr::Item(r) => r,
        ReferenceOr::Reference { reference } => {
            // Resolve #/components/responses/...
            let name = reference.strip_prefix("#/components/responses/")?;
            api.components
                .as_ref()?
                .responses
                .get(name)
                .and_then(|r| match r {
                    ReferenceOr::Item(item) => Some(item),
                    _ => None,
                })?
        }
    };

    let media = response.content.get("application/json")?;
    let schema_ref = media.schema.as_ref()?;
    Some(resolve_schema(schema_ref, api, 0))
}

fn resolve_schema(schema_ref: &ReferenceOr<Schema>, api: &OpenAPI, depth: usize) -> SchemaNode {
    if depth > 32 {
        return SchemaNode::Any;
    }

    let schema = match schema_ref {
        ReferenceOr::Item(s) => s,
        ReferenceOr::Reference { reference } => {
            if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                if let Some(components) = &api.components {
                    if let Some(s) = components.schemas.get(name) {
                        return resolve_schema(s, api, depth + 1);
                    }
                }
            }
            return SchemaNode::Any;
        }
    };

    convert_schema(schema, api, depth)
}

fn format_to_string<T: std::fmt::Debug>(fmt: &VariantOrUnknownOrEmpty<T>) -> Option<String> {
    match fmt {
        VariantOrUnknownOrEmpty::Item(v) => {
            // Map Debug names to OpenAPI-ish strings
            let s = format!("{v:?}").to_ascii_lowercase();
            Some(match s.as_str() {
                "datetime" => "date-time".into(),
                "date" => "date".into(),
                "int32" => "int32".into(),
                "int64" => "int64".into(),
                other => other.to_string(),
            })
        }
        VariantOrUnknownOrEmpty::Unknown(s) => Some(s.clone()),
        VariantOrUnknownOrEmpty::Empty => None,
    }
}

fn convert_schema(schema: &Schema, api: &OpenAPI, depth: usize) -> SchemaNode {
    match &schema.schema_kind {
        OasSchemaKind::Type(t) => match t {
            OasType::String(s) => SchemaNode::String {
                format: format_to_string(&s.format),
            },
            OasType::Number(_) => SchemaNode::Number,
            OasType::Integer(i) => SchemaNode::Integer {
                format: format_to_string(&i.format),
            },
            OasType::Boolean(_) => SchemaNode::Boolean,
            OasType::Array(arr) => {
                let items = arr
                    .items
                    .as_ref()
                    .map(|item| convert_boxed_item(item, api, depth))
                    .unwrap_or(SchemaNode::Any);
                SchemaNode::Array {
                    items: Box::new(items),
                }
            }
            OasType::Object(obj) => {
                let mut properties = IndexMap::new();
                for (key, prop_ref) in &obj.properties {
                    let node = match prop_ref {
                        ReferenceOr::Item(boxed_schema) => {
                            convert_schema(boxed_schema, api, depth + 1)
                        }
                        ReferenceOr::Reference { reference } => resolve_schema(
                            &ReferenceOr::Reference {
                                reference: reference.clone(),
                            },
                            api,
                            depth + 1,
                        ),
                    };
                    properties.insert(key.clone(), node);
                }
                SchemaNode::Object {
                    properties,
                    required: obj.required.clone(),
                    additional_properties: additional_properties_allowed(&obj.additional_properties),
                }
            }
        },
        OasSchemaKind::OneOf { .. }
        | OasSchemaKind::AllOf { .. }
        | OasSchemaKind::AnyOf { .. } => {
            if let OasSchemaKind::AllOf { all_of } = &schema.schema_kind {
                let mut properties = IndexMap::new();
                let mut required = Vec::new();
                for part in all_of {
                    if let SchemaNode::Object {
                        properties: p,
                        required: r,
                        ..
                    } = resolve_schema(part, api, depth + 1)
                    {
                        for (k, v) in p {
                            properties.insert(k, v);
                        }
                        required.extend(r);
                    }
                }
                if !properties.is_empty() {
                    return SchemaNode::Object {
                        properties,
                        required,
                        additional_properties: true,
                    };
                }
            }
            if let OasSchemaKind::OneOf { one_of } = &schema.schema_kind {
                if let Some(first) = one_of.first() {
                    return resolve_schema(first, api, depth + 1);
                }
            }
            if let OasSchemaKind::AnyOf { any_of } = &schema.schema_kind {
                if let Some(first) = any_of.first() {
                    return resolve_schema(first, api, depth + 1);
                }
            }
            SchemaNode::Any
        }
        OasSchemaKind::Any(any) => {
            if let Ok(val) = serde_json::to_value(any) {
                return json_value_to_schema(&val);
            }
            SchemaNode::Any
        }
        OasSchemaKind::Not { .. } => SchemaNode::Any,
    }
}

fn convert_boxed_item(
    item: &ReferenceOr<Box<Schema>>,
    api: &OpenAPI,
    depth: usize,
) -> SchemaNode {
    match item {
        ReferenceOr::Item(boxed) => convert_schema(boxed, api, depth + 1),
        ReferenceOr::Reference { reference } => resolve_schema(
            &ReferenceOr::Reference {
                reference: reference.clone(),
            },
            api,
            depth + 1,
        ),
    }
}

fn json_value_to_schema(val: &Value) -> SchemaNode {
    // Used as soft fallback for Any schemas
    if let Some(obj) = val.as_object() {
        if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
            return match t {
                "string" => SchemaNode::String {
                    format: obj
                        .get("format")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                },
                "integer" => SchemaNode::Integer {
                    format: obj
                        .get("format")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                },
                "number" => SchemaNode::Number,
                "boolean" => SchemaNode::Boolean,
                "array" => {
                    let items = obj
                        .get("items")
                        .map(json_value_to_schema)
                        .unwrap_or(SchemaNode::Any);
                    SchemaNode::Array {
                        items: Box::new(items),
                    }
                }
                "object" => {
                    let mut properties = IndexMap::new();
                    if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
                        for (k, v) in props {
                            properties.insert(k.clone(), json_value_to_schema(v));
                        }
                    }
                    let required = obj
                        .get("required")
                        .and_then(|r| r.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    SchemaNode::Object {
                        properties,
                        required,
                        additional_properties: obj
                            .get("additionalProperties")
                            .and_then(|v| v.as_bool())
                            .map(|allowed| allowed)
                            .unwrap_or(true),
                    }
                }
                _ => SchemaNode::Any,
            };
        }
    }
    SchemaNode::Any
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
openapi: "3.0.3"
info:
  title: Demo
  version: "1.0.0"
paths:
  /api/users:
    get:
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: "#/components/schemas/User"
    post:
      responses:
        "201":
          description: created
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/User"
  /api/users/{id}:
    get:
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/User"
components:
  schemas:
    User:
      type: object
      required: [id, user_email]
      properties:
        id:
          type: integer
        user_email:
          type: string
        created_at:
          type: string
          format: date-time
"##;

    #[test]
    fn parses_openapi_yaml() {
        let routes = parse(SAMPLE).unwrap();
        assert!(routes.len() >= 3);
        assert!(routes.iter().any(|r| r.path == "/api/users" && r.method == Method::GET));
        let get_users = routes
            .iter()
            .find(|r| r.path == "/api/users" && r.method == Method::GET)
            .unwrap();
        assert!(matches!(get_users.response_schema, SchemaNode::Array { .. }));
    }
}
