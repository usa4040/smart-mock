use crate::error::{AppError, Result};
use crate::schema::types::{collection_key_from_path, RouteInfo, SchemaNode};
use axum::http::Method;
use indexmap::IndexMap;
use std::collections::HashMap;

/// Parse a practical subset of TypeScript interfaces/types into CRUD routes.
pub fn parse(content: &str) -> Result<Vec<RouteInfo>> {
    let interfaces = extract_interfaces(content)?;
    if interfaces.is_empty() {
        return Err(AppError::Parse(
            "no export interface / type object definitions found".into(),
        ));
    }

    // First pass: build SchemaNodes with possible named refs left as placeholders
    let mut schemas: HashMap<String, SchemaNode> = HashMap::new();
    for (name, body) in &interfaces {
        schemas.insert(name.clone(), parse_object_body(body, &interfaces));
    }

    // Resolve named references inside schemas
    let resolved: HashMap<String, SchemaNode> = schemas
        .iter()
        .map(|(k, v)| (k.clone(), resolve_refs(v, &schemas, 0)))
        .collect();

    let mut routes = Vec::new();
    for name in interfaces.keys() {
        let schema = resolved
            .get(name)
            .cloned()
            .unwrap_or_else(SchemaNode::empty_object);
        let collection = to_collection_path(name);
        let item_path = format!("{collection}/{{id}}");

        routes.push(RouteInfo {
            method: Method::GET,
            path: collection.clone(),
            collection_key: collection_key_from_path(&collection),
            response_schema: SchemaNode::Array {
                items: Box::new(schema.clone()),
            },
            status: 200,
        });
        routes.push(RouteInfo {
            method: Method::POST,
            path: collection.clone(),
            collection_key: collection_key_from_path(&collection),
            response_schema: schema.clone(),
            status: 201,
        });
        routes.push(RouteInfo {
            method: Method::GET,
            path: item_path.clone(),
            collection_key: collection_key_from_path(&item_path),
            response_schema: schema.clone(),
            status: 200,
        });
        routes.push(RouteInfo {
            method: Method::PUT,
            path: item_path.clone(),
            collection_key: collection_key_from_path(&item_path),
            response_schema: schema.clone(),
            status: 200,
        });
        routes.push(RouteInfo {
            method: Method::DELETE,
            path: item_path,
            collection_key: collection.clone(),
            response_schema: SchemaNode::Any,
            status: 204,
        });
    }

    Ok(routes)
}

fn extract_interfaces(content: &str) -> Result<IndexMap<String, String>> {
    let mut result = IndexMap::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip comments
        if bytes[i] == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                continue;
            }
        }

        let rest = &content[i..];
        let (keyword, after_kw) = if rest.starts_with("export interface ") {
            ("interface", i + "export interface ".len())
        } else if rest.starts_with("interface ") {
            ("interface", i + "interface ".len())
        } else if rest.starts_with("export type ") {
            ("type", i + "export type ".len())
        } else if rest.starts_with("type ") {
            ("type", i + "type ".len())
        } else {
            i += 1;
            continue;
        };
        let _ = keyword;

        let name_start = after_kw;
        let name_end = content[name_start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .map(|n| name_start + n)
            .unwrap_or(content.len());
        if name_start >= name_end {
            i = after_kw;
            continue;
        }
        let name = content[name_start..name_end].to_string();

        // Find '=' for type alias or '{' for interface
        let mut j = name_end;
        while j < content.len() && content.as_bytes()[j].is_ascii_whitespace() {
            j += 1;
        }

        // Skip generics like <T>
        if j < content.len() && content.as_bytes()[j] == b'<' {
            let mut depth = 1;
            j += 1;
            while j < content.len() && depth > 0 {
                match content.as_bytes()[j] {
                    b'<' => depth += 1,
                    b'>' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            while j < content.len() && content.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
        }

        if j < content.len() && content.as_bytes()[j] == b'=' {
            j += 1;
            while j < content.len() && content.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
        }

        if j >= content.len() || content.as_bytes()[j] != b'{' {
            i = j.max(i + 1);
            continue;
        }

        // Extract balanced braces
        let body_start = j + 1;
        let mut depth = 1;
        j += 1;
        while j < content.len() && depth > 0 {
            match content.as_bytes()[j] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let body_end = j.saturating_sub(1);
        if body_start <= body_end {
            result.insert(name, content[body_start..body_end].to_string());
        }
        i = j;
    }

    Ok(result)
}

fn parse_object_body(body: &str, known: &IndexMap<String, String>) -> SchemaNode {
    let mut properties = IndexMap::new();
    let mut required = Vec::new();

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {
            continue;
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }

        // field?: Type  or  field: Type
        let (name_part, type_part) = match line.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        let optional = name_part.trim().ends_with('?');
        let field_name = name_part.trim().trim_end_matches('?').trim().to_string();
        if field_name.is_empty() {
            continue;
        }
        if !optional {
            required.push(field_name.clone());
        }
        let ty = type_part.trim();
        properties.insert(field_name, parse_ts_type(ty, known));
    }

    SchemaNode::Object {
        properties,
        required,
    }
}

fn parse_ts_type(ty: &str, known: &IndexMap<String, String>) -> SchemaNode {
    let ty = ty.trim().trim_end_matches(';').trim();

    // Array forms: Type[] or Array<Type>
    if let Some(inner) = ty.strip_suffix("[]") {
        return SchemaNode::Array {
            items: Box::new(parse_ts_type(inner.trim(), known)),
        };
    }
    if let Some(inner) = ty
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
    {
        return SchemaNode::Array {
            items: Box::new(parse_ts_type(inner.trim(), known)),
        };
    }

    // Inline object
    if ty.starts_with('{') && ty.ends_with('}') {
        let inner = &ty[1..ty.len() - 1];
        return parse_object_body(inner, known);
    }

    // Union → take first member
    if ty.contains('|') {
        let first = ty.split('|').next().unwrap_or(ty).trim();
        return parse_ts_type(first, known);
    }

    match ty {
        "string" => SchemaNode::String { format: None },
        "number" => SchemaNode::Number,
        "boolean" => SchemaNode::Boolean,
        "any" | "unknown" | "object" => SchemaNode::Any,
        "Date" => SchemaNode::String {
            format: Some("date-time".into()),
        },
        other => {
            if known.contains_key(other) {
                // Placeholder: store as object with a magic key for later resolve
                SchemaNode::Object {
                    properties: {
                        let mut m = IndexMap::new();
                        m.insert(
                            "__ref__".into(),
                            SchemaNode::String {
                                format: Some(other.to_string()),
                            },
                        );
                        m
                    },
                    required: vec![],
                }
            } else {
                SchemaNode::Any
            }
        }
    }
}

fn resolve_refs(node: &SchemaNode, schemas: &HashMap<String, SchemaNode>, depth: usize) -> SchemaNode {
    if depth > 16 {
        return SchemaNode::Any;
    }
    match node {
        SchemaNode::Object { properties, required } => {
            if properties.len() == 1 {
                if let Some(SchemaNode::String { format: Some(name) }) = properties.get("__ref__") {
                    if let Some(target) = schemas.get(name) {
                        return resolve_refs(target, schemas, depth + 1);
                    }
                }
            }
            let mut new_props = IndexMap::new();
            for (k, v) in properties {
                new_props.insert(k.clone(), resolve_refs(v, schemas, depth + 1));
            }
            SchemaNode::Object {
                properties: new_props,
                required: required.clone(),
            }
        }
        SchemaNode::Array { items } => SchemaNode::Array {
            items: Box::new(resolve_refs(items, schemas, depth + 1)),
        },
        other => other.clone(),
    }
}

fn to_collection_path(name: &str) -> String {
    let snake = camel_to_snake(name);
    let plural = pluralize(&snake);
    format!("/api/{plural}")
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn pluralize(s: &str) -> String {
    if s.ends_with('y') && s.len() > 1 {
        let prev = s.chars().rev().nth(1).unwrap_or('a');
        if !"aeiou".contains(prev) {
            return format!("{}ies", &s[..s.len() - 1]);
        }
    }
    if s.ends_with('s')
        || s.ends_with("x")
        || s.ends_with("z")
        || s.ends_with("ch")
        || s.ends_with("sh")
    {
        return format!("{s}es");
    }
    format!("{s}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_interface() {
        let content = r#"
export interface User {
  id: number;
  user_email: string;
  created_at: string;
}
"#;
        let routes = parse(content).unwrap();
        assert!(routes.iter().any(|r| r.path == "/api/users" && r.method == Method::GET));
        assert!(routes
            .iter()
            .any(|r| r.path == "/api/users/{id}" && r.method == Method::DELETE));
    }

    #[test]
    fn pluralize_basic() {
        assert_eq!(pluralize("user"), "users");
        assert_eq!(pluralize("category"), "categories");
    }
}
