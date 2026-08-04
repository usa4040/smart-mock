pub mod handler;
pub mod state;

use crate::schema::types::RouteInfo;
use crate::server::handler::fallback_handler;
use crate::server::state::AppState;
use axum::Router;
use std::net::SocketAddr;

/// Build the axum router with shared state.
pub fn build_router(routes: Vec<RouteInfo>) -> Router {
    let state = AppState::new(routes);
    Router::new()
        .fallback(fallback_handler)
        .with_state(state)
}

/// Run the mock server on the given port.
pub async fn serve(routes: Vec<RouteInfo>, port: u16) -> anyhow::Result<()> {
    let n = routes.len();
    let app = build_router(routes);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("smart-mock listening on http://localhost:{port} ({n} routes)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
