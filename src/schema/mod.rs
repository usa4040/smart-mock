use crate::error::{AppError, Result};
use crate::schema::types::{RouteInfo, RESET_PATH};
use std::path::Path;

pub mod openapi;
pub mod types;

pub use types::RESET_PATH as RESET_ROUTE;

/// Load an OpenAPI document from disk.
pub fn load_routes(path: &Path) -> Result<Vec<RouteInfo>> {
    let content = std::fs::read_to_string(path)?;
    parse_openapi(&content)
}

/// Parse an OpenAPI v3 document. Other schema kinds are outside the product.
pub fn parse_openapi(content: &str) -> Result<Vec<RouteInfo>> {
    if !looks_like_openapi(content) {
        return Err(AppError::UnsupportedFormat(
            "smart-mock reads an OpenAPI document".into(),
        ));
    }
    let routes = openapi::parse(content)?;
    if routes.iter().any(|route| route.path == RESET_PATH) {
        return Err(AppError::Validation(format!(
            "contract path {RESET_PATH} is reserved for resetting the sandbox"
        )));
    }
    Ok(routes)
}

fn looks_like_openapi(content: &str) -> bool {
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            return false;
        };
        return value.get("openapi").is_some() || value.get("paths").is_some();
    }
    trimmed.contains("openapi:") || trimmed.contains("paths:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_json_route_definition() {
        let error = parse_openapi(r#"{"routes":[]}"#).unwrap_err();
        assert!(matches!(error, AppError::UnsupportedFormat(_)));
    }

    #[test]
    fn rejects_reset_collision() {
        let spec = r#"
openapi: "3.0.3"
info: { title: t, version: "1" }
paths:
  /__smart_mock/reset:
    post:
      responses:
        "204":
          description: no content
"#;
        let error = parse_openapi(spec).unwrap_err();
        assert!(matches!(error, AppError::Validation(_)));
    }
}
