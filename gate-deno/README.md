# gate-deno

A lightweight API gateway in TypeScript using Deno. Zero dependencies, single file, true response streaming.

## Quick Start

```bash
PROXY='[{"name":"api","ip":"httpbin.org","port":443,"tls":true}]' deno run --allow-net --allow-env main.ts
```

## Configuration

Set the `PROXY` env var with a JSON array of services:

```json
[
  { "name": "users", "ip": "users-service", "port": 8080 },
  { "name": "orders", "ip": "orders-service", "port": 8080, "api-key": "secret" },
  { "name": "external", "ip": "api.example.com", "port": 443, "tls": true, "timeout": 60, "host": "api.example.com" }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Service name used in URL path |
| `ip` | yes | Upstream host or IP |
| `port` | yes | Upstream port |
| `api-key` | no | Require `X-API-KEY` header to access this service |
| `tls` | no | Use HTTPS to connect upstream |
| `timeout` | no | Request timeout in seconds (default: 30) |
| `host` | no | Override the Host header sent upstream |

## Routing

```
GET  http://localhost:8080/{name}/path  ->  http://{ip}:{port}/path
POST http://localhost:8080/{name}/path  ->  http://{ip}:{port}/path
```

All HTTP methods are supported (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS).

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check (`{"status":"ok","version":"1.0.0"}`) |
| `GET /services` | List all registered services |
| `/{name}/**` | Proxy to the named service |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PROXY` | `[]` | JSON array of service configs |
| `PORT` | `8080` | Listen port |

## Docker

```bash
./build.sh 1.0.0
docker run -d -p 8080:8080 -e PROXY='[...]' gate-deno:1.0.0
```

## Kubernetes

```bash
kubectl apply -f k8s/
```

## Test

```bash
./test.sh                        # test localhost:8080
./test.sh http://myhost:9090     # test custom host
```
