# Configure an installed App from your own application

Read and update declared installation settings through HTTP, MCP or the existing
JavaScript SDK. Secret settings are encrypted in PostgreSQL and never returned.
The standalone service runs without Veyndo Core; native Veyndo uses the same
settings transaction, projection and original authenticated encryption envelope.

## Explicit operator setup

Start with the dedicated [reader schema](README.md) and
[managed write upgrade](managed-writes.md). As its original migration owner run:

```sh
psql "$APP_DATA_MIGRATION_URL" -X -v reader_role=app_data_reader \
  -f data-service/sql/enable-settings.sql
```

This upgrades schema 2 to 3. It adds one fixed `app_service_secrets` table and the
native package `digest` registration column. It expands the existing minimized
outbox contract for `app.secret_rotated`. The serving login can select/insert
settings and update ciphertext, digest, revision, actor and rotation time. It
cannot change tenant/App/kind keys, delete settings, install packages, read the
outbox or create SQL objects. Signed forced RLS remains active. Startup checks
reject missing or excessive grants and mismatched profiles. No per-App tables or
indexes are created.

The trusted installation owner must validate/provision complete immutable
packages and register `model::manifest::canonical_digest(&manifest)` in
`app_packages.digest`. Existing rows default to an empty registration and cannot
save settings until that owner registers their exact current package. A settings request
cannot change this column. The separate opt-in [installation profile](installation.md)
can review and install portable private data packages; schema 3 alone cannot.

Set these private deployment values alongside the existing App Data configuration:

| Variable | Value |
| --- | --- |
| `APP_DATA_MODE` | `write` |
| `APP_DATA_SETTINGS` | `enabled` |
| `APP_DATA_SETTINGS_KEY` | Separate operator key of 64 random hexadecimal characters |

Keep the encryption key in the operator's secret manager and backup procedure.
Never reuse the signed SQL key. Restart with the same encryption key to read
existing configuration. This release provides no key-reencryption endpoint.
Disabled settings and read-only profiles never adopt these extra grants silently.

## Current personal administrator

Each request performs fresh introspection with the existing fixed OAuth owner.
Besides the current merchant/audience/tenant/subject/expiry context, settings need
`apps.read apps.manage team.manage`, `personal_session: true` and `live: true`.
The identity owner must derive these nondelegatable claims from the actual
personal live administrator session, never from scope requests or App input.
Missing claims default to false. Machine keys, callback tokens, delegated agents,
shoppers and private stages cannot manage these settings. No second role store,
issuer, cached consent or browser-supplied identity is introduced.

These are host management operations. Their HTTP and MCP adapters share this
administrator admission, independent of per-App data action MCP opt-in. Using
MCP never relaxes personal identity or explicit approval requirements.

## Read settings

```http
POST /v1/settings/read
Authorization: Bearer <current personal resource access token>
Content-Type: application/json

{"app":"advice"}
```

The response contains `fields` with multilingual labels, visible nonsecret
`values`, configured key names, `revision`, `digest`, `current`, `canManage` and
`encryptionReady`. A secret value, ciphertext or vault key never appears here.
Original empty strings, false and zero remain distinct. Older-package values are
hidden with `current: false`; the user must review and supply current settings.

## Save an explicit partial update

```http
POST /v1/settings/save
Authorization: Bearer <current personal resource access token>
Content-Type: application/json

{"app":"advice","digest":"<digest from read>","revision":0,"approve":true,
 "values":{"key":"<new private App value>","enabled":false,"limit":0}}
```

Omitted declared values are preserved only for the same package. Required values
cannot be cleared. Unknown keys/types/control characters, oversized payloads,
false approval and stale digest/revision reject. Shared encrypted CAS, actor,
package lock and metadata-only outbox run in one transaction. A late failure rolls
everything back. A conflict returns 409; reload and review before retrying.
After a network timeout, read the current revision before retrying an uncertain
write. Encryption authenticates tenant, App, kind, package and revision.

## Existing SDK and MCP

MCP `/mcp` advertises `apps.settings.read` and `apps.settings.save` only in the
enabled settings profile. `/v1/contract` and `/v1/openapi.json` expose the same
strict wire schemas. The SDK requires no service-specific fork:

```js
await client.initialize();
const settings = await client.tool('apps.settings.read', {app: 'advice'});
await client.tool('apps.settings.save', {
  app: 'advice', digest: settings.digest, revision: settings.revision,
  approve: true, values: {enabled: false}
});
```

[Executable SDK and Fetch example](examples/settings.mjs) is the actual synthetic
fixture workflow. `tests/acceptance.py` runs the real Rust process and PostgreSQL
with two shops, current authority, CAS races, package changes, late rollback,
wrong keys, cross-shop ciphertext rejection and persistence across restart.
Container CI repeats it using the nonroot Rust-only image. The standalone installation profile is a separate schema-4 opt-in. Settings alone
do not provide transport-secret rotation, jobs, internal
runtime setting resolution, restore or complete standalone release orchestration.
