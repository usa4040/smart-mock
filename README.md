# Smart Mock

Stateful local mock server for an OpenAPI document. Point it at a contract and `POST /api/users` shows up on the next `GET /api/users`. Requests that violate the contract fail.

## Quick start

```bash
cargo run -- ./examples/openapi.yaml -p 4000
# GET http://localhost:4000/api/users
# POST http://localhost:4000/api/users
```

`POST /__smart_mock/reset` clears the in-memory records. The next list `GET` seeds them again from the response example, or from three generated items when the contract has no example.

## What it records

A collection is a path with no parameters paired with the same path ending in `{id}`, whose list response is a JSON array. Other operations, including lists wrapped in an object, return an example or a generated value and are not recorded.

The published URL is the path written in `paths`. `servers` is not prefixed.

## License

MIT
