pub mod context;

use crate::schema::types::SchemaNode;
use fake::faker::internet::en::SafeEmail;
use fake::faker::lorem::en::{Sentence, Word};
use fake::faker::name::en::Name;
use fake::Fake;
use rand::Rng;
use serde_json::{json, Map, Value};

const DEFAULT_ARRAY_LEN: usize = 3;

/// Generate a JSON value from a schema node.
pub fn generate(schema: &SchemaNode) -> Value {
    generate_with_name(schema, "")
}

pub fn generate_with_name(schema: &SchemaNode, field_name: &str) -> Value {
    match schema {
        SchemaNode::Object { properties, .. } => {
            let mut map = Map::new();
            for (key, node) in properties {
                if key == "__ref__" {
                    continue;
                }
                map.insert(key.clone(), generate_with_name(node, key));
            }
            Value::Object(map)
        }
        SchemaNode::Array { items } => {
            let values: Vec<Value> = (0..DEFAULT_ARRAY_LEN)
                .map(|_| generate_with_name(items, field_name))
                .collect();
            Value::Array(values)
        }
        SchemaNode::String { format } => {
            Value::String(context::string_for_field(field_name, format.as_deref()))
        }
        SchemaNode::Integer { format } => {
            json!(context::integer_for_field(field_name, format.as_deref()))
        }
        SchemaNode::Number => json!(context::number_for_field(field_name)),
        SchemaNode::Boolean => {
            let mut rng = rand::thread_rng();
            Value::Bool(rng.gen())
        }
        SchemaNode::Any => {
            if !field_name.is_empty() {
                Value::String(context::string_for_field(field_name, None))
            } else {
                json!({})
            }
        }
    }
}

/// Generate a single item for a collection (unwraps array schema).
pub fn generate_item(schema: &SchemaNode) -> Value {
    generate(schema.item_schema())
}

/// Generate N items for seeding a collection.
pub fn generate_items(schema: &SchemaNode, count: usize) -> Vec<Value> {
    (0..count).map(|_| generate_item(schema)).collect()
}

/// Ensure an object has an `id` field suitable for store lookups.
pub fn ensure_id(mut value: Value, next_int_id: i64) -> Value {
    if let Value::Object(ref mut map) = value {
        if !map.contains_key("id") {
            map.insert("id".into(), json!(next_int_id));
        }
    }
    value
}

/// Extract a stringified id from a JSON value, if present.
pub fn extract_id(value: &Value) -> Option<String> {
    value.get("id").map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    })
}

#[allow(dead_code)]
pub fn sample_email() -> String {
    SafeEmail().fake()
}

#[allow(dead_code)]
pub fn sample_name() -> String {
    Name().fake()
}

#[allow(dead_code)]
pub fn sample_word() -> String {
    Word().fake::<String>()
}

#[allow(dead_code)]
pub fn sample_sentence() -> String {
    Sentence(3..8).fake::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn generates_email_for_user_email() {
        let mut props = IndexMap::new();
        props.insert(
            "user_email".into(),
            SchemaNode::String { format: None },
        );
        props.insert("id".into(), SchemaNode::Integer { format: None });
        let schema = SchemaNode::Object {
            properties: props,
            required: vec![],
        };
        let value = generate(&schema);
        let email = value["user_email"].as_str().unwrap();
        assert!(email.contains('@'), "expected email, got {email}");
    }
}
