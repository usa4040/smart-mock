use crate::error::{AppError, Result};
use crate::schema::types::RouteInfo;
use std::path::Path;

pub mod json_def;
pub mod openapi;
pub mod typescript;
pub mod types;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaKind {
    OpenApi,
    JsonDef,
    TypeScript,
}

/// Detect schema kind from file extension and/or content.
pub fn detect_kind(path: &Path, content: &str) -> Result<SchemaKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "ts" | "tsx" => return Ok(SchemaKind::TypeScript),
        "yaml" | "yml" => return Ok(SchemaKind::OpenApi),
        "json" => {
            let trimmed = content.trim_start();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if v.get("routes").is_some() {
                    return Ok(SchemaKind::JsonDef);
                }
                if v.get("openapi").is_some() || v.get("paths").is_some() {
                    return Ok(SchemaKind::OpenApi);
                }
            }
            return Err(AppError::UnsupportedFormat(
                "JSON file must be OpenAPI (openapi/paths) or a routes definition".into(),
            ));
        }
        _ => {}
    }

    // Fallback: sniff content
    let trimmed = content.trim_start();
    if trimmed.contains("interface ") || trimmed.contains("type ") && trimmed.contains('{') {
        return Ok(SchemaKind::TypeScript);
    }
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if v.get("routes").is_some() {
                return Ok(SchemaKind::JsonDef);
            }
            if v.get("openapi").is_some() || v.get("paths").is_some() {
                return Ok(SchemaKind::OpenApi);
            }
        }
    }
    if trimmed.contains("openapi:") || trimmed.contains("paths:") {
        return Ok(SchemaKind::OpenApi);
    }

    Err(AppError::UnsupportedFormat(format!(
        "cannot detect schema kind for {}",
        path.display()
    )))
}

/// Load and parse a schema file into route definitions.
pub fn load_routes(path: &Path) -> Result<Vec<RouteInfo>> {
    let content = std::fs::read_to_string(path)?;
    let kind = detect_kind(path, &content)?;
    match kind {
        SchemaKind::OpenApi => openapi::parse(&content),
        SchemaKind::JsonDef => json_def::parse(&content),
        SchemaKind::TypeScript => typescript::parse(&content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_json_def() {
        let content = r#"{"routes":[]}"#;
        assert_eq!(
            detect_kind(Path::new("schema.json"), content).unwrap(),
            SchemaKind::JsonDef
        );
    }

    #[test]
    fn detect_openapi_json() {
        let content = r#"{"openapi":"3.0.0","paths":{}}"#;
        assert_eq!(
            detect_kind(Path::new("api.json"), content).unwrap(),
            SchemaKind::OpenApi
        );
    }

    #[test]
    fn detect_ts() {
        assert_eq!(
            detect_kind(Path::new("users.ts"), "export interface User {}").unwrap(),
            SchemaKind::TypeScript
        );
        let _ = PathBuf::new();
    }
}
