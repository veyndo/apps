# Veyndo Apps: model and persistence foundations

The native Veyndo app record and job storage now has an independent Rust package.
`veyndo-app-storage` accepts an existing PostgreSQL connection and explicit
`Scope`/`RecordKey` values. It has no dependency on the Veyndo application, app
runtime, identity tables, inference, provider clients or environment configuration.

This is the first storage extraction inside the broader Apps component. The full
standalone app host, authenticated permission adapter, API/MCP service and storage
cutover remain on the [component task list](https://github.com/veyndo/veyndo/blob/main/docs/commerce-components.md).
The library is a trusted host port, not a browser API or an authentication boundary.

## Independent Wasm runtime

[`veyndo-app-runtime`](runtime/) executes the actual core-Wasm/WIT hooks independently.
It retains isolated stores, bounded fuel/memory/host calls and required host resource
admission. The Core adapter supplies its verified predicate; there is no dependency
on the Core or kernel crate. Run the [quote example](runtime/examples/quote.rs) without
SQL or a Veyndo process. Current identities, snapshots and applying results remain
host responsibilities.

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

## Reusable model contracts

| Capability | Contract and host boundary |
| --- | --- |
| Complete standard compiler | [All 19 domain stages and mandatory host policies](model/README.md#complete-standard-package-compiler), with an [independent example](runtime/examples/package_compile.rs); current identity, installation and effects stay host-owned. |
| Complete package wire, publisher and provider contracts | [Strict Manifest, fingerprints, dependency graphs and domain grammar](model/README.md#complete-package-wire-and-domain-contracts), with an [executable signing example](examples/package_contracts.rs); current trust, identity and effects stay host-owned. |
| Settings and schema evolution | [Strict updates, secret-safe projections and explicit upgrades](model/README.md#settings-and-explicit-model-upgrades); encryption and transactions stay with the host. |
| Events and triggers | [Subscriptions, privacy, filters, schedules and webhooks](model/README.md#events-schedules-and-incoming-webhooks); URL policy, signatures and queues stay with the host. |
| Object bindings and graph | [Product/customer/order/company declarations and graph views](model/README.md#native-object-bindings-and-graph-views); actual object rights and scoped lists stay with the host. |
| Placements and HTTP routes | [Strict surface/route grammar](model/README.md#placements-and-named-http-routes); native views, known team scopes and read-only effects require trusted host policies. |
| Native views and UI behavior | [Typed controls, geometry, editor bindings and bounded behavior](model/README.md#native-views-and-ui-behavior); actual execution, field policies and current grants stay with the host. |
| Native order and quote lifecycles | [Bounded graph compilation and accepted snapshots](model/README.md#native-order-and-quote-lifecycle-contracts), with an [executable example](examples/native_lifecycles.rs); current native rights and effects stay with the host. |
| Configuration and commerce examples | [Strict business contracts](model/README.md#configuration-and-executable-business-contracts) and [actual independent preflight](runtime/README.md#business-preflight); host policies, current record/package ownership and effects remain attached. |

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
The complete typed manifest/domain compiler is now shared through
`model::compiler`; identity and standalone service remain tracked extraction work. See `model/README.md` and its executable Rust tests.

## Durable job history and control

Release 0.17.0 adds `job_history` for bounded, scoped keyset pages and detail reads,
and `job_control` for the original cancel/retry/resolve/archive transaction. The
component owns the row lock, revision comparison, explicit unknown-outcome review,
lease cleanup, retry actor/package rebind and atomic queued/control outbox writes.
`RetryAdmission` is mandatory: the host checks the current installed job action,
current primary/additional rights and stored input before returning its event.
No implicit retries, second queue, pool creation or internal commit are introduced.

History returns trusted-host rows, including the package fingerprint needed for
redaction. Before returning them over HTTP/MCP, the host must check current package,
actor and action rights, clear forbidden messages/results and remove the internal
fingerprint. A correlation group never grants access. Every control error requires
rollback of the caller's transaction, including hook writes.

After the disposable PostgreSQL contract test has installed its test schema, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example job_control
```

[The example](examples/job_control.rs) executes a synthetic failure, acknowledged
retry, cancellation and archival using the actual component, then rolls its entire
transaction back. Its fixed synthetic admission is not a production identity policy.
The database tests cover scope/hostile labels, tied/cross-tenant/stale cursors,
uncertainty projections, concurrent CAS, denied admission and hook/outbox rollback.
Creator-role revalidation, signed callback identities, completion evidence and the
verified completion predicate remain with the existing host owners.

## Encrypted app settings and credentials

Release 0.18.0 supplies `secrets` for metadata-only reads, digest-bound internal
resolution and the original revisioned encrypted rotation transaction. A required
`Crypto` port supplies authenticated encryption and key management; there is no
plaintext or approving fallback. Veyndo delegates to its existing platform owner.
The component retains `app:tenant:app:kind:digest:revision` binding, scoped locking,
CAS, active-package admission and atomic metadata-only outbox. Revision overflow
returns an explicit invalid-input error. Caller transactions own commit/rollback.
`resolve_locked` keeps the original webhook verification/receipt row lock through
rotation races. Current personal administrator admission, declared kinds/settings,
consent, sandbox exclusion, payload rules and browser redaction stay host-owned.

After the disposable database contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example encrypted_settings
```

[The example](examples/encrypted_settings.rs) uses a fresh ephemeral test-only
AES-GCM key, runs model preparation, storage, internal resolution and secret-safe
projection, then rolls everything back. It prints no key, value or ciphertext.
Production hosts must supply their own admitted identity, supported schema and
real key owner. No production key provisioning, pool/schema creation or implicit
transaction commit is provided by the library.

## Immutable Studio versions and private staging

Release 0.19.0 adds `build_versions`: immutable development snapshot persistence,
original serialized-byte digests, bounded active/archive history, recoverable
archive/restore and the actual row-fenced staging sequence. Native Core uses the
same component. The development digest deliberately retains its original JSON
serialization contract; it is not substituted by the installed-package digest.
Archived version identities remain immutable. Archive/restore never changes
installed packages, app records or published versions.

Staging requires `StageAdmission`, with no approving default. The native adapter
checks owned private environment, typed package/compiler and current actor scopes,
then calls the actual shared installation path on the same original transaction.
The component marks a build staged only after that succeeds. All failures require
caller rollback of package/model/version/event writes and build state. Current
HTTP/MCP approval, role/tenant scope and privileged connection stay host-owned.

History rows are trusted host data: retained prompts are consumed only by the
host's minimized design-evidence projection. Never serialize raw rows or expose
prompts to public clients. The component does not authenticate actors, create a
schema/pool, choose an environment or commit transactions.

After the disposable PostgreSQL contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example build_versions
```

[The example](examples/build_versions.rs) executes save/history/archive/restore
and proves staging is denied without an authorized installer, then rolls back.
The PostgreSQL regression exercises successful staging through the actual shared
installer with explicit synthetic hooks, failures at admission/install/state
write, foreign/archive/digest denial, concurrent immutable saves and staging
versus archival. These fixture policies are not a production identity/compiler.
