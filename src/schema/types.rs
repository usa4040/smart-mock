use axum::http::Method;
use indexmap::IndexMap;

/// Internal representation of a JSON Schema-like node used for data generation.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaNode {
    Object {
        properties: IndexMap<String, SchemaNode>,
        required: Vec<String>,
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

/// A single HTTP route derived from the input schema.
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: Method,
    /// Path template, e.g. `/api/users/{id}`
    pub path: String,
    /// Collection key used for the in-memory store, e.g. `/api/users`
    pub collection_key: String,
    pub response_schema: SchemaNode,
    pub status: u16,
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
