# Veyndo App Runtime service

Run a commerce app function from your own application or AI agent through HTTP,
MCP or the Veyndo JavaScript SDK. Supply a Wasm component and example cart,
product and app-record values. The service evaluates price, shipping, discount
and validation hooks using the same bounded engine that Veyndo embeds.

It runs as an independent Rust process with your OAuth identity provider. No
Veyndo server or database is needed. Every result states `mode: "simulation"`
and `effectApplied: false`, and includes exact source and input fingerprints.
Use the ordinary commerce owner to review and apply a proposal to a real shop.

## Start

From the Apps repository root:

```sh
cargo build --locked --manifest-path service/Cargo.toml
# Supply these through your existing deployment secret configuration.
export APP_RUNTIME_INTROSPECTION_URL=https://identity.example/oauth/introspect
export APP_RUNTIME_INTROSPECTION_TOKEN='<resource-server credential>'
export APP_RUNTIME_AUDIENCE=veyndo-app-runtime
export APP_RUNTIME_BIND=127.0.0.1:8798
service/target/debug/veyndo-app-runtime-service
```

`APP_RUNTIME_INTROSPECTION_TOKEN` authorizes this service at the fixed identity
endpoint. A caller presents its own OAuth access token to the runtime. The two
credentials have separate purposes. Use server-side environment injection or your
secret manager; keep them out of Git, browser code, screenshots and command logs.

The service checks current authorization for every protected request through
[OAuth token introspection, RFC 7662](https://www.rfc-editor.org/rfc/rfc7662.html).
Its form contains `token`, `token_type_hint=access_token` and a fixed `resource`
audience. The identity endpoint receives the service credential as Bearer auth.
There is no token-result cache, key issuance, user/role store or fallback identity.
The trusted identity response must contain:

```json
{
  "active": true,
  "sub": "current-user",
  "tenant": "current-workspace",
  "scope": "apps.evaluate",
  "aud": "veyndo-app-runtime",
  "exp": 1794000000
}
```

`exp` is the actual future Unix expiry, not the example above. `aud` may also be
an array containing the configured audience. `tenant` is an explicit provider
extension identifying the current authorized workspace. `sub` and `tenant` use
1–128 ASCII letters, digits or `_ - . : @`. Configure the provider to evaluate
current workspace access and scopes. A role change or revocation takes effect on
the next request. Caller headers, arguments and snapshots cannot supply rights.
Redirects, bad/oversized identity responses and unavailable identity fail closed
with a sanitized 503; inactive/expired/wrong-audience tokens return 401 and missing
`apps.evaluate` returns 403.

Identity URLs require HTTPS. Plain HTTP is accepted only for literal loopback
addresses while the runtime itself binds to loopback, for local development.
Optional `APP_RUNTIME_ORIGINS` is a JSON array of exact allowed HTTPS origins.
An absent list denies every request with an Origin header. This follows the
[MCP HTTP Origin checks](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).
The service does not expose a browser credential or general CORS gateway.

## HTTP, OpenAPI and MCP

All operations except `GET /health` require current access.

| Interface | Operation |
| --- | --- |
| `POST /v1/evaluations` | Typed simulation input and result |
| `GET /v1/contract` | Version, input/output schemas, limits and MCP tool |
| `GET /v1/openapi.json` | OpenAPI 3.1 projection of those same schemas |
| `POST /mcp` | Stateless JSON MCP: initialize, ping, tools/list and tools/call |

The MCP tool is `apps.evaluate`. Protocol versions 2025-11-25, 2025-06-18 and
2025-03-26 are supported. Accepted initialization notifications receive 202;
there is no session store or SSE stream. JSON-RPC ID/protocol/parameter errors
remain distinct from guest tool failures. HTTP and MCP call the same operation
and return the same structured result. All responses use `Cache-Control: no-store`.

The input has exactly `source`, `hook`, `snapshot` and `context`. `source` is the
WIT component text, `hook` is price/shipping/discount/validation, `context` is a
JSON object, and `snapshot` is the shared typed frozen example model. Its subtotal,
items and products preserve integer minor amounts and consistent currency/scale;
record fields are supplied JSON objects. No product/record ID is dereferenced.
The schemas describe the wire shape; cross-field currency, uniqueness and byte
budgets are enforced by the same operation.

Errors identify invalid requests, component compilation/resource admission, WIT
interface rejection, trapped/budget-exhausted execution, identity failure or busy
capacity. They do not return private identity errors, credentials, guest source or
snapshot contents. Results retain the typed bounded outcome and fingerprints.

## JavaScript and agent examples

Use a Node 22+ server process with a caller token:

```sh
export APP_RUNTIME_URL=http://127.0.0.1:8798
export APP_RUNTIME_TOKEN='<caller access token>'
node service/examples/evaluate.mjs runtime/tests/fixtures/snapshot-price.component.wat
```

The example uses ordinary Fetch, evaluates a 25,000 minor-unit cart and checks
structural HTTP/MCP parity. The existing [Veyndo SDK](https://github.com/veyndo/sdk)
works unchanged, including its default MCP negotiation:

```sh
# Check out veyndo/sdk beside this repository.
export APP_RUNTIME_SDK_MODULE="$(pwd)/../sdk/src/index.js"
node service/examples/sdk.mjs runtime/tests/fixtures/snapshot-price.component.wat
```

An agent discovers the same typed input/output schemas through `tools/list`.
Read-only tool hints describe behavior; every invocation still needs current
access. No model-generated role or tool argument grants authority.

## Execution and process limits

The shared runtime retains fresh stores, no WASI, no network/filesystem/SQL imports,
WIT ABI checks, fuel/epoch deadlines, memory/table/stack limits and bounded output.
This adapter admits the current `veyndo:commerce@1.0.0` namespace only. Existing
native Veyndo namespace compatibility retains its original operator configuration.

Before compilation, input bounds use the shared frozen snapshot validator.
Limits include 32 KiB source, 8 KiB context, four synthetic app records with at
most 64 KiB combined field JSON, 100 products/items and a 128 KiB HTTP body.
Execution admission permits four tasks globally, two per current tenant and one
per current subject. Task-owned permits survive client timeout/disconnect until
the actual blocking computation finishes. Idle identity keys are removed, so
capacity tracking does not grow with past tenants. Access checks allow at most
16 in-flight requests; identity HTTP has a five-second deadline and subsequent
request handling has a 15-second deadline. Capacity rejection includes Retry-After.

These are per-process limits. Multiple replicas need ingress/fleet admission.
The engine boundary does not provide a separate operating-system process per
guest. Results are synthetic evaluations, not installation, publication, database
provisioning or an approved live financial effect.

## Container and checks

```sh
docker build -f service/Dockerfile -t veyndo-app-runtime .
# Provide the configured identity variables through your deployment secret manager.
docker run --rm -p 127.0.0.1:8798:8798 \
  -e APP_RUNTIME_INTROSPECTION_URL -e APP_RUNTIME_INTROSPECTION_TOKEN \
  -e APP_RUNTIME_AUDIENCE veyndo-app-runtime
```

The runtime image uses UID/GID 10001 and contains the Rust server and TLS roots,
with no Rust compiler or Python. It opens no database connection. Build caches
retain Cargo dependencies and compiled targets independently of documentation.

```sh
cargo clippy --locked --manifest-path service/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path service/Cargo.toml
APP_RUNTIME_SDK_MODULE="$(pwd)/../sdk/src/index.js" python3 service/tests/http_mcp.py
```

The process fixture starts a local identity server and the actual Rust binary,
then exercises real Wasm through HTTP, MCP, Fetch and the pinned existing SDK.
It covers immediate revocation, denied/malformed/provider responses, origin and
duplicate authorization, strict inputs, money/snapshot/source limits, tool errors
and redaction. A separate cancellation test keeps the actual blocking task alive
after its waiting request is aborted and proves capacity remains occupied.
Linux container CI repeats the process fixture against the built non-root image.
Container builds run daily, manually or when the Docker recipe changes; the
focused service tests run on component changes. Complete Veyndo tests retain their
existing daily cadence.

App data lifecycle, supported standalone database provisioning and the full
installation/control API remain on the [component plan](https://github.com/veyndo/veyndo/blob/main/docs/commerce-components.md).
