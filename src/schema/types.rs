use axum::http::Method;
use indexmap::IndexMap;
use serde_json::Value;

/// Internal representation of a JSON Schema-like node used for data generation.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaNode {
    Object {
        properties: IndexMap<String, SchemaNode>,
        required: Vec<String>,
        /// When false, properties outside `properties` are a spec violation.
        additional_properties: bool,
    },
    Array {
        items: Box<SchemaNode>,
    },
    String {
        format: Option<String>,
    },
    Integer {
        format: Option<String>,
    },
    Number,
    Boolean,
    Any,
}

impl SchemaNode {
    pub fn empty_object() -> Self {
        SchemaNode::Object {
            properties: IndexMap::new(),
            required: Vec::new(),
            additional_properties: true,
        }
    }

    /// Prefer the item schema when this node is an array (useful for collection seeding).
    pub fn item_schema(&self) -> &SchemaNode {
        match self {
            SchemaNode::Array { items } => items,
            other => other,
        }
    }
}

pub const RESET_PATH: &str = "/__smart_mock/reset";

/// How a missing id is minted for a recorded collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Integer,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamIn {
    Query,
    Header,
    Path,
    Cookie,
}

#[derive(Debug, Clone)]
pub struct RequestParam {
    pub location: ParamIn,
    pub name: String,
    pub required: bool,
    pub schema: SchemaNode,
}

/// One way to prove auth. A requirement is an OR of AND-groups of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthScheme {
    AuthorizationHeader,
    ApiKeyHeader(String),
    ApiKeyQuery(String),
    ApiKeyCookie(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRequirement {
    /// No credential is required. Includes a security list that contains `{}`.
    None,
    /// Caller must satisfy one group, and every scheme inside that group.
    Required(Vec<Vec<AuthScheme>>),
}

/// A single HTTP route derived from an OpenAPI document.
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: Method,
    /// Path template, e.g. `/api/users/{id}`
    pub path: String,
    /// Collection key used for the in-memory store, e.g. `/api/users`
    pub collection_key: String,
    /// This route reads or writes a recorded JSON-array collection.
    pub recorded: bool,
    pub response_schema: SchemaNode,
    pub request_schema: Option<SchemaNode>,
    pub status: u16,
    /// `application/json` example, if the operation declares one.
    pub example: Option<Value>,
    pub auth: AuthRequirement,
    pub params: Vec<RequestParam>,
    pub id_kind: IdKind,
}

/// Derive a collection key by stripping trailing path parameters.
pub fn collection_key_from_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return path.trim_end_matches('/').to_string();
    }

    let last = segments[segments.len() - 1];
    let is_param = last.starts_with('{') && last.ends_with('}')
        || last.starts_with(':')
        || last == "*";

    let kept = if is_param {
        &segments[..segments.len() - 1]
    } else {
        &segments[..]
    };

    if kept.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", kept.join("/"))
    }
}

/// Returns true if the path template ends with a parameter segment.
pub fn is_item_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.last().is_some_and(|last| {
        (last.starts_with('{') && last.ends_with('}')) || last.starts_with(':')
    })
}

/// Match a request path against a route template. Returns captured params on success.
pub fn match_path(template: &str, request_path: &str) -> Option<IndexMap<String, String>> {
    let t_segs: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();
    let r_segs: Vec<&str> = request_path.split('/').filter(|s| !s.is_empty()).collect();

    if t_segs.len() != r_segs.len() {
        return None;
    }

    let mut params = IndexMap::new();
    for (t, r) in t_segs.iter().zip(r_segs.iter()) {
        if t.starts_with('{') && t.ends_with('}') {
            let name = &t[1..t.len() - 1];
            params.insert(name.to_string(), r.to_string());
        } else if t.starts_with(':') {
            let name = &t[1..];
            params.insert(name.to_string(), r.to_string());
        } else if *t != *r {
            return None;
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_key_strips_param() {
        assert_eq!(collection_key_from_path("/api/users/{id}"), "/api/users");
        assert_eq!(collection_key_from_path("/api/users"), "/api/users");
        assert_eq!(collection_key_from_path("/api/users/:id"), "/api/users");
    }

    #[test]
    fn match_path_captures_id() {
        let params = match_path("/api/users/{id}", "/api/users/42").unwrap();
        assert_eq!(params.get("id").map(String::as_str), Some("42"));
        assert!(match_path("/api/users/{id}", "/api/posts/42").is_none());
    }
}
