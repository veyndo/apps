# Veyndo Apps: model and persistence foundations

The native Veyndo app record and job storage now has an independent Rust package.
`veyndo-app-storage` accepts an existing PostgreSQL connection and explicit
`Scope`/`RecordKey` values. It has no dependency on the Veyndo application, app
runtime, identity tables, inference, provider clients or environment configuration.

This is the first storage extraction inside the broader Apps component. The full
standalone app host, permission adapter, complete package compiler, API/MCP service and storage
cutover remain on the [component task list](https://github.com/veyndo/veyndo/blob/main/docs/commerce-components.md).
The library is a trusted host port, not a browser API or an authentication boundary.

## Pure model package

[`veyndo-app-model`](model/) has no SQL or network dependency. It exports the exact
entity/field/extension wire types, declaration grammar, bounded action schemas,
field value checks, enabled-language rules, publication contracts and record
workflows. Native currency, rich-content, relation-shape and core-reference policy
checks enter through explicit required ports. Veyndo implements those ports with
its existing owners; callers cannot select a permissive policy through HTTP/MCP.
Model checks do not authenticate users or resolve ownership of referenced objects.

## Host authority ports

The model admission module shares private-merchant checks, primary/additional
rights, callback capability mappings and callback-key scope planning. A trusted
host implements `Authority`/`ScopeAuthority` from its current authenticated
principal. Request-body roles, tenant IDs and permission arrays never create
that authority. Callback consent facts remain separate from current role facts;
Veyndo applies its existing verified callback predicate to both.

The storage package also owns the original active-package shared lock/read and
canonical JSON fingerprint. The host still validates the full installed package,
checks its expected digest, authorizes reserved/provider packages and owns install,
upgrade, revocation, secret storage and commit. This is not a token issuer or a
standalone authentication service.

Settings declarations/updates, secret-safe projections and explicit schema evolution
planning are also shared through [the pure model package](model/README.md#settings-and-explicit-model-upgrades).
Encryption, current authority and actual transactional migrations stay with the host.

The [pure model event contracts](model/README.md#events-schedules-and-incoming-webhooks)
also share subscription/privacy rules, projected filters, bounded delivery and
schedule/webhook declarations. URL policy is required; actual delivery, signatures,
current authority and queues remain host-owned.

[Object bindings and graph contracts](model/README.md#native-object-bindings-and-graph-views)
share product/customer/order/company declaration rules and selected-field graph
projections. Actual object rights, scoped lists and verified release admission
remain host responsibilities.

## What is shared with Veyndo

- Fixed-table record get/put, bounded model scans, model definitions and precisely
  scoped preview cleanup. The native caller retains field/reference validation,
  CAS, language inheritance, history, quotas, signed RLS and publication rules.
- The shared record write pipeline locks one scoped record, checks CAS and import
  replay, invokes mandatory host workflow/history hooks and writes the minimized
  `app.record.changed` event in the caller's transaction. A host error requires
  rollback; no transaction is committed by this package. Revision overflow is
  rejected explicitly.
- Bounded keyset pages reuse shared typed equality indexes. Public status/channel/
  schedule predicates use bound field names and values. The host admits declared
  index fields and projects only currently visible fields before returning rows.
- Durable app job enqueue with the original tenant-wide serialization, request
  fingerprint, package/action binding, quotas and one atomic job/outbox handoff.
- Scoped job reads, single-winner claim and lease-fenced progress persistence.
  Unknown or expired jobs are never automatically reclaimed by this library.
- Existing transactions remain with the caller. A rejection rolls back record,
  history, projection and event changes together in the native host.

Veyndo consumes an exact source snapshot from this repository. Source history was
extracted from `src/apps`; [provenance](PROVENANCE.json) records the original commit.
The original [license](LICENSE) is retained. There is no alternate order ledger,
pricing engine or queue implementation.

## Use in a trusted Rust host

Add a Git dependency pinned to the reviewed release commit, then use the existing
transaction. The host authenticates the actor and admits the exact package first.

```rust,ignore
use veyndo_app_storage::{RecordKey, Scope, records};
let scope = Scope { tenant: admitted_tenant, app: admitted_app };
let key = RecordKey { scope, entity: "offers", id: admitted_id };
let mut tx = pool.begin().await?;
let current = records::get(&mut tx, key, true).await?;
// Validate current revision, fields, references and rights in the admitted host.
records::put(&mut tx, key, next_revision, &validated_fields).await?;
// Write history/projections/events in this same transaction before committing.
tx.commit().await?;
```

Never construct admission from a request's tenant field alone or hand a database
connection to a plugin, agent or untrusted client. `Scope` is a storage key, not
proof of authorization. The package does not open pools, install schemas, acquire
administrator rights, change RLS, execute services or automatically commit.

## Verify without Veyndo

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo test --locked --test postgres -- --ignored --nocapture
```

The PostgreSQL contract test requires an explicitly selected empty disposable
database and installs `tests/schema.sql` there. This minimum test schema contains
no Veyndo identity or commerce tables. It verifies composite record scope,
transaction rollback, atomic job/outbox handoff, concurrent idempotency, changed
request rejection, claims, progress/cancellation fences, quotas, connection restart
and scoped preview cleanup. Additional cases cover record CAS races, replay,
host hook/outbox rollback, typed filters, publication and hostile bound values.
CI runs it against actual PostgreSQL 17.

The minimum test schema is not a production app schema or replacement for the
native signed RLS, references, model validation and storage accounting. Native
integration tests remain required when composing a new component version.

## Atomic installation in the host transaction

`installation::install` takes the caller's `sqlx::Transaction`, admitted package
metadata and mandatory `InstallHooks`. Hook stages preserve the original order:
prepare compiler/commerce checks; acquire tenant/package locks and quota; admit
dependencies; check immutable versions and run the host migration; persist package
and attach models; record immutable version and event; seed configuration/schedules.
The returned digest comes from the same canonical JSON as package write fences.

Implement every hook from the host's real policy owners. In particular, `upgrade`
must parse/check the old contract, reject unsupported downgrades and migrate data.
`versions: false` is solely for a host-authorized private mutable preview; admission
must never accept that choice from untrusted request fields. The component does
not authenticate a request, validate the full package, approve a provider, create
a production schema or commit the transaction. Roll back on every error.

The actual PostgreSQL example in `tests/cases/installation.rs` exercises all five
hooks, failure rollback, version isolation and preview behavior. Its companion
concurrency test checks two competing installs at the 128-package quota and a
package upgrade blocked by a writer. Test hooks demonstrate the port contract; a
production host must supply its own real compiler, permission and commerce owners.

## Shared package compiler

Release 0.6.0 also exposes the independent model 0.3.0 package compiler. Its common
metadata/model/action/slot/event grammar runs between mandatory host extension
validators; reference, scope and Wasm-kind policies are required ports. Native
Veyndo invokes this same compiler for its real Studio/API/MCP/release consumers.
The complete typed manifest/domain compiler, identity and standalone service remain
tracked extraction work. See `model/README.md` and its executable Rust tests.
