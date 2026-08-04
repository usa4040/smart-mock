use crate::error::{AppError, Result};
use crate::schema::types::{collection_key_from_path, RouteInfo, SchemaNode};
use axum::http::Method;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct JsonDef {
    routes: Vec<JsonRoute>,
}

#[derive(Debug, Deserialize)]
struct JsonRoute {
    path: String,
    method: String,
    #[serde(default)]
    response: Option<Value>,
    #[serde(default)]
    status: Option<u16>,
}

/// Parse a simple JSON route definition.
pub fn parse(content: &str) -> Result<Vec<RouteInfo>> {
    let def: JsonDef =
        serde_json::from_str(content).map_err(|e| AppError::Parse(e.to_string()))?;

    let mut routes = Vec::with_capacity(def.routes.len());
    for route in def.routes {
        let method = parse_method(&route.method)?;
        let schema = route
            .response
            .as_ref()
            .map(value_to_schema)
            .unwrap_or_else(SchemaNode::empty_object);
        let default_status = match method {
            Method::POST => 201,
            Method::DELETE => 204,
            _ => 200,
        };
        routes.push(RouteInfo {
            method,
            collection_key: collection_key_from_path(&route.path),
            path: route.path,
            response_schema: schema,
            status: route.status.unwrap_or(default_status),
        });
    }
    Ok(routes)
}

fn parse_method(s: &str) -> Result<Method> {
    match s.to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "DELETE" => Ok(Method::DELETE),
        "PATCH" => Ok(Method::PATCH),
        other => Err(AppError::Parse(format!("unsupported HTTP method: {other}"))),
    }
}

pub fn value_to_schema(val: &Value) -> SchemaNode {
    let obj = match val.as_object() {
        Some(o) => o,
        None => return SchemaNode::Any,
    };

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("object");
    match ty {
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
                .map(value_to_schema)
                .unwrap_or(SchemaNode::Any);
            SchemaNode::Array {
                items: Box::new(items),
            }
        }
        "object" | _ => {
            let mut properties = IndexMap::new();
            if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
                for (k, v) in props {
                    properties.insert(k.clone(), value_to_schema(v));
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_def() {
        let content = r#"{
          "routes": [
            {
              "path": "/api/users",
              "method": "GET",
              "response": {
                "type": "array",
                "items": {
                  "type": "object",
                  "properties": {
                    "id": { "type": "integer" },
                    "user_email": { "type": "string" }
                  }
                }
              }
            }
          ]
        }"#;
        let routes = parse(content).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/api/users");
        assert!(matches!(routes[0].response_schema, SchemaNode::Array { .. }));
    }
}
