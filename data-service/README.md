# Veyndo App Data service

Read installed App data from your own application or agent through HTTP, MCP or
JavaScript. This Rust service runs without a Veyndo Core process. It shares the
actual native bounded query preparation, declared-field projection and signed
PostgreSQL scope protocol with Veyndo.

## Optional managed writes

Service 0.2.0 adds an explicitly enabled managed-write profile with package/revision
fences, current action rights, typed private records, state transitions, atomic
history/outbox and the same SDK/MCP operations. [Enable and use managed writes](managed-writes.md).
The default read profile below keeps its original SELECT-only schema and grants.

## Optional encrypted settings

Service 0.3.0 adds the explicit schema-3 settings profile, shared with native Veyndo.
[Configure and use encrypted settings](settings.md). The ordinary reader and writer
profiles retain their existing schema/grants and do not expose settings.

## Optional portable package installation

Service 0.4.0 adds explicit schema-4 package review and installation through
HTTP, MCP and the existing SDK. [Install a private data App](installation.md).
It uses the complete compiler and shared native installation/model sequence,
with immutable version history and original per-App row/byte budgets.

## Optional job supervision

Service 0.5.0 adds a deliberate schema-5 supervisor for existing App jobs:
[history, details and approved cancel/retry/resolve/archive](jobs.md), through
HTTP, MCP and the unchanged SDK. Current authority, result redaction, lease/CAS
fences and atomic events share their native owners. Trusted producers and worker
execution remain separate; the supervisor has no job creation or result endpoint.

## Run

Build from the Apps repository root:

```sh
cargo build --locked --manifest-path data-service/Cargo.toml
```

Provision a fresh dedicated PostgreSQL database and a distinct existing restricted
login. The serving login must be LOGIN, NOINHERIT, NOSUPERUSER, NOBYPASSRLS,
NOCREATEDB, NOCREATEROLE and NOREPLICATION, with no role membership or owned objects.
Use your ordinary secret manager to create its password. Run the schema installer
once as the migration owner, with an existing role name:

```sh
psql "$APP_DATA_MIGRATION_URL" -X -v reader_role=app_data_reader -f data-service/sql/install.sql
```

The installer creates one version marker and three shared tables: packages,
models and records. Every model uses these same tables and composite keys.
It configures forced RLS with the native signed scope verifier. It revokes PUBLIC
CREATE/TEMP database access, then grants the reader only CONNECT, schema USAGE
and SELECT on those four tables. Use a separate database; this installer deliberately
refuses existing native schema objects. It is never invoked on server startup.

Use the migration owner's existing `public.veyndo_scope_key()` to place the active
key ID and secret directly in your deployment secret configuration. Never expose
them in an API response, frontend or terminal log. The serving role cannot read
the signing key table or function result. Configure:

| Variable | Value |
| --- | --- |
| `APP_DATA_DATABASE_URL` | Restricted reader connection, with TLS for remote databases |
| `APP_DATA_DATABASE_ROLE` | Actual reader login name |
| `APP_DATA_SCOPE_KEY_ID` | Active 32 hex signing key ID |
| `APP_DATA_SCOPE_KEY` | Active 64 hex signing secret |
| `APP_DATA_INTROSPECTION_URL` | Fixed HTTPS RFC 7662 endpoint |
| `APP_DATA_INTROSPECTION_TOKEN` | Private resource credential, 32 to 4096 graphic ASCII bytes |
| `APP_DATA_AUDIENCE` | Distinct OAuth resource audience for App Data |
| `APP_DATA_BIND` | Default `127.0.0.1:8799` |
| `APP_DATA_ORIGINS` | JSON array of exact allowed browser origins; default `[]` |

The fixed identity owner must resolve current membership and scopes, and return
`active`, `sub`, `tenant`, `scope`, `aud`, `exp`, plus `merchant: true` for a current
merchant identity. A shopper token cannot read this private endpoint. Tenant and
subject must be canonical IDs of 1 to 64 ASCII letters, digits, underscores or
hyphens, matching the signed database contract. Require `apps.read` plus the
installed action's current primary and additional rights. Missing merchant context
denies access. Browser/request fields never supply identity, tenant or rights.

Key changes require a coordinated private configuration update and reader restart.
The reader fails closed when its key no longer verifies. It does not operate a key
rotation endpoint, own an OAuth issuer, cache grants or create accounts.

## Read an installed action

```http
POST /v1/records/query
Authorization: Bearer <current resource access token>
Content-Type: application/json

{"app":"advice","action":"list_notes","query":{"limit":20,"after":"note_001"}}
```

The optional query keys are `limit` (1 to 100, default 100), `after` (up to 100
bytes) and `id` (1 to 100 bytes). Unknown properties, caller tenant fields, writes,
indexed filters and public storefront reads are rejected by this reader contract.
The installed private `list` action's original input schema is also checked.
Pagination returns `elements`, `limit`, `hasMore` and `nextCursor`. Only currently
declared fields, record IDs and revisions are projected; original localized maps,
empty strings and null values are preserved. Declared graph projections use the
same App model owner. No second knowledge graph is persisted here.

MCP `/mcp` exposes `apps.records.read`, backed by exactly the same operation.
It additionally requires `mcp: true` on that installed action. Supported protocol
versions are 2025-11-25, 2025-06-18 and 2025-03-26. Use `/v1/contract` or
`/v1/openapi.json` for the authenticated contract. `/health` is public and contains
no tenant or record data. All responses are `no-store`; errors omit database,
identity-provider and credential details.

Each read uses one signed REPEATABLE READ READ ONLY transaction. Package activation
and the records come from the same snapshot. A package disabled or a user revoked
before the next request is denied then; an already admitted read is allowed to
finish its snapshot. Maximum eight database connections, eight concurrent reads,
two-second SQL statements and 16 KiB input bound resource use. No row locks or
write grants are needed.

## Existing SDK and Fetch

See [the executable example](examples/sdk.mjs). The ordinary `@veyndo/sdk`
client initializes MCP and calls `client.tool('apps.records.read', input)`; it
returns the record page directly. Fetch uses the HTTP request above. The example
requires `APP_DATA_URL`, `APP_DATA_TOKEN` and `APP_DATA_SDK_MODULE` pointing at a
checkout of `veyndo/sdk/src/index.js`. These are the actual acceptance calls.

## Install and native composition boundary

This is the supported **reader schema and service**, not the complete independent
app lifecycle host. Packages/models/records must be provisioned by a trusted
installation owner after its full package compiler, field/reference and migration
checks. There is no public seed shortcut. The optional installation profile above supplies
reviewed portable package admission; ordinary profiles have no install authority. The local
test creates synthetic fixtures as its separate migration owner. Native Veyndo
continues using its existing actual installer, shared storage and transaction owner.

Models extending or referencing product, customer, order or company are rejected
with `native_resolver_required` until the real authorized object resolver is wired.
This cannot be bypassed by adding more token scopes. Managed private writes/history and encrypted settings have explicit opt-in profiles
above. Transport-secret rotation, jobs, restore, complete native lifecycle orchestration
and durable marketplace hosting remain separate checklist tasks.

## Verification and image

```sh
cargo clippy --locked --manifest-path data-service/Cargo.toml --all-targets -- -D warnings
APP_DATA_TEST_ADMIN_URL=postgres://fixture@127.0.0.1/test_database \
APP_DATA_SDK_MODULE=/absolute/path/sdk/src/index.js python3 data-service/tests/acceptance.py
docker build -f data-service/Dockerfile -t veyndo-app-data .
```

Acceptance creates and removes a unique synthetic test database and reader role.
It runs the actual Rust service, real fixed identity endpoint and PostgreSQL,
checking two shops, current rights/revocation, package disable, SQL forgery, wrong
signing keys, restricted startup, field projection, pagination and HTTP/MCP/SDK.
The container uses a nonroot Rust binary with no Python, Cargo or migration tools.
CI also runs the same acceptance against that container with a read-only root
filesystem, no capabilities and no new privileges.
