# Smart Mock

Context-aware local Mock API server written in Rust. Build mock endpoints from OpenAPI, TypeScript interfaces, or a simple JSON definition — no Node.js required.

## Quick start

```bash
cargo run -- ./examples/openapi.yaml -p 4000
# GET http://localhost:4000/api/users
```

```bash
cargo run -- ./examples/schema.json
cargo run -- ./examples/users.ts --port 3000
```

## Features

- OpenAPI v3 (JSON / YAML), TypeScript `interface` / object `type`, and JSON route definitions
- Field-name heuristics (`user_email`, `created_at`, `price_jpy`, …) via the `fake` crate
- In-memory stateful CRUD (`POST` / `PUT` / `DELETE` reflected on later `GET`s)
- Single binary powered by `axum` + `tokio`

## License

MIT
