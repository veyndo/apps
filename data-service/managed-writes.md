# Managed App record writes and retained history

App Data service 0.2.0 adds an explicit optional write profile. Run portable private
App records independently through HTTP, MCP and the existing Veyndo SDK. The same
record mutation, state transition and bounded history implementation is embedded
by native Veyndo. Default `APP_DATA_MODE=read` preserves schema 1 and its SELECT-only
login. A read host never silently acquires write privileges.

## Enable deliberately

Install the dedicated reader database first as described in the [setup](README.md).
Then, as its original migration owner, run:

```sh
psql "$APP_DATA_MIGRATION_URL" -X -v reader_role=app_data_reader -f data-service/sql/enable-writes.sql
```

This explicit upgrade requires schema 1 and the configured existing login. It adds
two fixed tables, `app_record_versions` and `outbox`, then sets schema version 2.
The four original application tables and the two fixed authority tables remain.
No per-App/model/shop table or index is created.

Set `APP_DATA_MODE=write` and restart using that same restricted nonowner login and
private scope key. Its new grants are limited to record INSERT/field-and-revision
UPDATE, history SELECT/INSERT/retention DELETE, outbox INSERT and sequence USAGE.
Package row locking needs UPDATE on the active column syntactically; the RLS policy
has `WITH CHECK false`, so every actual package edit by this login is denied. It
has no package installation, model/schema, record deletion or history update rights.
Startup checks both the positive and forbidden grants. Serving code never runs
migrations, accepts an owner connection or mints system database tickets.

This profile requires a separate operator-controlled upgrade. Switching the process
back to read mode with the expanded writer grants fails startup. To return to a
SELECT-only deployment, provision/revoke its profile through the operator's normal
credential/schema workflow; there is no automatic downgrade or destructive cleanup.

## Review and save

Read the private list action first. In the write profile, its response also contains
`packageDigest`. This binds the reviewed request to the installed package. Then:

```http
POST /v1/records/write
Authorization: Bearer <current App Data access token>
Content-Type: application/json

{"app":"cases","action":"save_case","packageDigest":"<reviewed 64 hex digest>","approve":true,"input":{"id":"case_1","revision":0,"fields":{"title":"New case","reference":"stable","status":"new"}}}
```

A new record uses revision 0; an update supplies the current revision. Explicit
`approve: true` confirms the requested mutation, but grants no authority. The
current identity must be a merchant with `apps.read`, `apps.manage` and the
installed private save action's primary/additional scopes. Caller tenant/actor/
permissions/package definitions are not accepted. The installed input schema,
exact typed fields and any declared output schema are checked.

One signed transaction locks the active package and record, checks the package
digest and revision, validates App-owned states and immutable identity fields,
captures retained history, writes the record and minimizes `app.record.changed`
to App/entity/record/revision metadata. It validates the output before commit.
An error rolls all these effects back. Concurrent updates to the same revision
have one winner; the other receives `revision_conflict`. Repeated network writes
are not automatically retried: inspect the current record/revision after an
uncertain response.

## Supported record policy

The initial portable write profile supports private declarative App-owned records
with string, integer, boolean, JSON, exact decimal string, date and datetime fields,
declared choices/validation and App-owned workflow states. Fields remain bounded
by the actual shared model validator; request size is 128 KiB and stored fields
are at most 64 KiB. The ordinary read profile retains its 16 KiB request bound.

Native object extensions, native references, assets/files, rich text, money,
relations, translations, indexed/unique fields, public content, operational records
and commerce configuration/hook/provider/lifecycle contracts require their actual
host owners and are rejected with `native_policy_required`. This profile does not
replace those validators with permissive defaults. It cannot edit the native
site builder through its generic records path. The trusted installer must still
admit the full package; this service exposes no install/seed shortcut.

## History and agent use

```http
POST /v1/records/history
Authorization: Bearer <current App Data access token>
Content-Type: application/json

{"app":"cases","action":"list_cases","id":"case_1"}
```

The current private list action's rights and the read scopes retained with each
historical version must both hold. Only current declared fields are returned.
Actor/source come from the authenticated host context. History is immutable and
uses the original 20-version / 16 MiB App retention rules. No historical restore
endpoint is claimed by this release.

MCP exposes `apps.records.write` and `apps.records.history` only in the enabled
profile. The selected installed action additionally requires `mcp: true`; HTTP
retains its own action admission. The same inputs work through the generic SDK:

```js
const page = await client.tool('apps.records.read', {app:'cases', action:'list_cases', query:{}});
const saved = await client.tool('apps.records.write', {
  app:'cases', action:'save_case', packageDigest:page.packageDigest, approve:true,
  input:{id:'case_1', revision:0, fields:{title:'New case', reference:'stable', status:'new'}}
});
const history = await client.tool('apps.records.history', {app:'cases', action:'list_cases', id:saved.id});
```

The [executable SDK example](examples/write.mjs) runs against the real service in
acceptance. OpenAPI and `/v1/contract` describe the enabled profile. Identity is
resolved freshly for every request; MCP opt-in never substitutes for current rights.

## Acceptance boundary

The actual Rust service, fixed identity endpoint and real PostgreSQL test both
profiles with two shops. Tests cover positive and forbidden role grants, package
row locks without package edits, approval/digest/revision failures, CAS races,
states/immutable fields, denied native policies, old history rights and retention,
late event/history/output failures with full rollback, reconnect and SDK/MCP.
The same suite runs against the nonroot Rust container on Linux.

This is managed portable record hosting. Complete independent package installation,
settings, jobs, actual native resource policies, historical restore, sandbox
publication and an autonomous outbox/Flow consumer remain separately tracked.
Native Veyndo keeps all those existing host owners and uses the shared mutation
and lifecycle/history hooks on its original transaction.
