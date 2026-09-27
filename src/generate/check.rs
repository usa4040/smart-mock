use crate::schema::types::SchemaNode;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    /// Required properties are enforced. `id` is satisfied elsewhere.
    Write,
    /// Only the fields present in the body are checked.
    Patch,
}

pub fn check_body(schema: &SchemaNode, value: &Value, mode: CheckMode) -> Result<(), String> {
    check_value(schema, value, "", mode, true)
}

fn check_value(
    schema: &SchemaNode,
    value: &Value,
    path: &str,
    mode: CheckMode,
    exempt_id: bool,
) -> Result<(), String> {
    match schema {
        SchemaNode::Object {
            properties,
            required,
            additional_properties,
        } => {
            let Value::Object(map) = value else {
                return Err(format!("{} expected object", label(path)));
            };
            if !*additional_properties {
                for key in map.keys() {
                    if !properties.contains_key(key) {
                        return Err(format!("unexpected property {key}"));
                    }
                }
            }
            if mode == CheckMode::Write {
                for name in required {
                    if exempt_id && name == "id" {
                        continue;
                    }
                    if !map.contains_key(name) || map.get(name).is_some_and(Value::is_null) {
                        return Err(format!("missing required property {name}"));
                    }
                }
            }
            for (key, node) in properties {
                if let Some(child) = map.get(key) {
                    if child.is_null() {
                        continue;
                    }
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    check_value(node, child, &child_path, mode, false)?;
                }
            }
            Ok(())
        }
        SchemaNode::Array { items } => {
            let Value::Array(values) = value else {
                return Err(format!("{} expected array", label(path)));
            };
            for (index, child) in values.iter().enumerate() {
                check_value(items, child, &format!("{path}[{index}]"), mode, false)?;
            }
            Ok(())
        }
        SchemaNode::String { .. } => {
            if value.is_string() {
                Ok(())
            } else {
                Err(format!("{} expected string", label(path)))
            }
        }
        SchemaNode::Integer { .. } => {
            if value.as_i64().is_some() || value.as_u64().is_some() {
                Ok(())
            } else {
                Err(format!("{} expected integer", label(path)))
            }
        }
        SchemaNode::Number => {
            if value.is_number() {
                Ok(())
            } else {
                Err(format!("{} expected number", label(path)))
            }
        }
        SchemaNode::Boolean => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err(format!("{} expected boolean", label(path)))
            }
        }
        SchemaNode::Any => Ok(()),
    }
}

fn label(path: &str) -> &str {
    if path.is_empty() {
        "body"
    } else {
        path
    }
}

pub fn query_matches(schema: &SchemaNode, raw: &str) -> bool {
    match schema {
        SchemaNode::Integer { .. } => raw.parse::<i64>().is_ok(),
        SchemaNode::Number => raw.parse::<f64>().is_ok(),
        SchemaNode::Boolean => matches!(raw, "true" | "false"),
        SchemaNode::Array { .. } | SchemaNode::Object { .. } => false,
        SchemaNode::String { .. } | SchemaNode::Any => true,
    }
}
