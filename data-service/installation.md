# Install a private data App without Veyndo Core

Review a complete App package, explicitly approve it, configure its settings and
use its declared records from your own application. HTTP, MCP and the existing
JavaScript SDK call the same Rust operations. Native Veyndo shares the compiler,
installation transaction and fixed-table model installer.

## Enable installation explicitly

Provision the dedicated [reader](README.md), [managed writes](managed-writes.md)
and [encrypted settings](settings.md) in that order. As their original migration
owner, upgrade schema 3 to 4:

```sh
psql "$APP_DATA_MIGRATION_URL" -X -v reader_role=app_data_reader \
  -f data-service/sql/enable-installation.sql
```

Keep the serving connection on the existing restricted nonowner login. This
upgrade adds two fixed tables, `app_versions` and `app_storage_usage`, and narrow
package/model write grants. It preserves prior registered versions and initializes
usage from actual existing records. It creates no table or index per App, shop or
model. Schema tools and migration credentials never enter the serving image.

Add these values to the existing settings/write configuration:

| Variable | Value |
| --- | --- |
| `APP_DATA_INSTALLS` | `enabled` |
| `APP_DATA_INSTALL_SCOPES` | Operator-controlled JSON array, for example `["catalog.read","catalog.write"]` |

The scope catalogue names rights the existing identity owner actually understands.
A package cannot register new rights or grant them to its caller. Every operation
uses fresh fixed-endpoint introspection. Review and installation require
`apps.read apps.manage team.manage`, a current merchant identity and actual
`personal_session: true`, `live: true` claims from that owner. Delegated agents,
machine keys and shopper identities cannot install. The ordinary reader, writer
and settings profiles retain their previous schema and do not advertise install
tools. Startup rejects profile mismatches, excessive grants, missing quota
triggers or an owner/bypass serving login.

## Review the complete package

Start with the [complete example package](tests/portable-package.json). It declares
its `app_` namespace, version, private models, permissions, settings and bounded
list/save action schemas. The complete native compiler checks the package before
either operation. This profile supports portable private data Apps with scalar
fields, list/save actions and declared installation settings. Native references,
core extensions, indexes, public surfaces, commerce hooks, arbitrary code,
schedules and schema migrations require their real host policies and are rejected
here. Their native implementation stays with its existing owner.

```http
POST /v1/packages/review
Authorization: Bearer <current personal resource access token>
Content-Type: application/json

{"manifest":{...complete package...}}
```

Review writes nothing. It returns the canonical `digest`, the package `revision`
(zero for a new App), declared `permissions`, previous version and model/settings
counts. It checks actual installed model compatibility and immutable version
history. Compiler errors include their original message and field pointer where
available. HTTP and MCP reject caller tenant, identity or environment properties.

## Approve and install exactly what was reviewed

```js
await client.initialize();
const review = await client.tool('apps.packages.review', {manifest});
const installed = await client.tool('apps.packages.install', {
  manifest,
  digest: review.digest,
  revision: review.revision,
  permissions: review.permissions,
  approve: true
});
```

HTTP uses `POST /v1/packages/install` with those same fields. The exact digest,
current revision and complete permission list are mandatory. The package is
compiled again at installation. Original install locks, the 128 Apps per shop
limit, package CAS, model compatibility, immutable version, usage initialization
and minimized `app.installed` event commit together. A late failure rolls the
whole transaction back. Concurrent approval yields one winner; reload and review
after a 409 conflict. After an uncertain network result, review current state
before retrying.

## Configure and use the installed App

The installed actions are immediately available through `apps.records.read` and
`apps.records.write` with their current declared rights. The latter requires the
installed digest, explicit approval and record revision. Declared secrets use the
existing encrypted `apps.settings.read` / `apps.settings.save` workflow. Neither
review nor a settings read returns secret values.

Run the [executable SDK journey](examples/install.mjs) against a fresh test shop:

```sh
APP_DATA_URL=http://127.0.0.1:8799 \
APP_DATA_TOKEN='current-personal-resource-token' \
APP_DATA_SDK_MODULE=/absolute/path/sdk/src/index.js \
node data-service/examples/install.mjs
```

The example reviews, installs and writes/reads the new model through the unchanged
generic SDK. MCP advertises the two management tools only when enabled.
`/v1/contract` version 4 and `/v1/openapi.json` expose the same schemas.

## Upgrade and storage behavior

Use a higher version for additive changes. Published version identities and their
digests are immutable. Removing models/fields, changing field kinds or adding a
required field without migration is rejected. Existing records retain their IDs,
values and revisions. A changed installed package makes old settings noncurrent;
review and configure the new version explicitly.

Forced signed RLS isolates shops. Database grants prevent package identity edits,
version rewrites and direct usage-counter changes. Original limits of 100,000
records and 64 MiB per App apply to the same fixed record tables. Accounting and
record model versions run inside the actual record transaction, so exceeding
either budget leaves record, history and event unchanged. This service creates
no OAuth issuer, second role store, second commerce ledger or SQL objects for
individual Apps.

`tests/acceptance.py` exercises the actual Rust process, PostgreSQL, fixed identity
endpoint, MCP and SDK, including two shops, revocation, approval errors, competing
installs, additive upgrades, late rollback, immutable versions, quota boundaries
and restart. Native Veyndo keeps its full resource-aware installation and sandbox
publication path; this independent profile does not yet host jobs, restore,
transport-secret rotation or native release orchestration.
