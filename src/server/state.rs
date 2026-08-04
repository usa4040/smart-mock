use crate::schema::types::RouteInfo;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type StoreInner = HashMap<String, Vec<Value>>;

#[derive(Clone)]
pub struct AppState {
    pub routes: Arc<Vec<RouteInfo>>,
    pub store: Arc<RwLock<StoreInner>>,
}

impl AppState {
    pub fn new(routes: Vec<RouteInfo>) -> Self {
        Self {
            routes: Arc::new(routes),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn next_id(&self, collection: &str) -> i64 {
        let store = self.store.read().await;
        let max = store
            .get(collection)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.get("id"))
                    .filter_map(|id| match id {
                        Value::Number(n) => n.as_i64(),
                        Value::String(s) => s.parse().ok(),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        max + 1
    }
}
