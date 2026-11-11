# Veyndo Apps: independent runtime and data services

The native Veyndo app record and job storage now has an independent Rust package.
`veyndo-app-storage` accepts an existing PostgreSQL connection and explicit
`Scope`/`RecordKey` values. It has no dependency on the Veyndo application, app
runtime, identity tables, inference, provider clients or environment configuration.

The [standalone Rust runtime service](service/) now exposes authenticated HTTP,
OpenAPI and MCP evaluation using the same bounded engine. The existing Veyndo SDK
connects unchanged. It uses your current OAuth identity provider and supplied
example data, without a Veyndo process or database. See its executable Fetch/SDK
examples and independent real-process/container checks.

The [App Data reader](data-service/) adds independent HTTP/MCP/SDK access to
private installed App records on a versioned fixed PostgreSQL schema. Current
OAuth rights, forced signed RLS and a restricted read-only login are enforced.
It shares actual native page/field rules and the [host authority adapter](authority/).
The [managed write profile](data-service/managed-writes.md) adds explicitly
approved package-bound CAS changes, original states, retained history and atomic
outbox for portable private records. Full installation/native-policy/jobs hosting
and storage cutover remain on the [component task list](https://github.com/veyndo/veyndo/blob/main/docs/commerce-components.md).
The storage library is a trusted host port, not a browser authentication boundary.

## Independent Wasm runtime

[`veyndo-app-runtime`](runtime/) executes the actual core-Wasm/WIT hooks independently.
It retains isolated stores, bounded fuel/memory/host calls and required host resource
admission. The Core adapter supplies its verified predicate; there is no dependency
on the Core or kernel crate. Run the [quote example](runtime/examples/quote.rs) without
SQL or a Veyndo process. Current identities, snapshots and applying results remain
host responsibilities.

## Pure model package

Model 0.14.0 supplies `query_contract::list_input_schema` for discoverable native
list filters. Agent and MCP hosts can expose the actual indexed fields and choice
values instead of an untyped filter object. This adds no database indexes or
query authority. See the [model contract](model/README.md).

[`veyndo-app-model`](model/) has no SQL or network dependency. It exports the exact
entity/field/extension wire types, declaration grammar, bounded action schemas,
field value checks, enabled-language rules, publication contracts and record
workflows. Native currency, rich-content, relation-shape and core-reference policy
checks enter through explicit required ports. Veyndo implements those ports with
its existing owners; callers cannot select a permissive policy through HTTP/MCP.
Model checks do not authenticate users or resolve ownership of referenced objects.

## Install a private data App independently

Apps 0.28.0 / Data service 0.4.0 can review and install a complete private data
package through HTTP, OpenAPI, MCP and the existing SDK, without a Veyndo Core
process. The full compiler, exact digest/permission/revision confirmation,
original installer/model sequence, immutable versions and fixed shared storage
form one transaction. Current personal live administrator authority is required.
[Setup and executable SDK journey](data-service/installation.md).

Native Veyndo also calls shared `models::install`, retaining its actual native
compatibility policy and full installation owner. Unsupported standalone native
resources, service effects and UI do not acquire approving fallback policies.

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

Release 0.27.0 adds shared `settings::read/save` orchestration and the opt-in
[standalone settings API, MCP and JavaScript workflow](data-service/settings.md).
Native Veyndo and the independent host use the same partial updates, encrypted
CAS persistence, package binding and secret-safe projection. Optional authority
`vault` provides the original platform AES-GCM envelope without environment, SQL
or network access. Personal live administrator admission and private key loading
remain host responsibilities.


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

## Private mutable Studio drafts

Release 0.20.0 adds `drafts`: bounded actor-private autosaves, history projection,
CAS updates and scoped deletion, using the original fixed `developer_drafts`
table. No per-app, per-model, per-shop or per-version SQL objects are created.
The native product delegates to this exact implementation. Mutable drafts remain
separate from immutable versions, installed packages and live publication.

The trusted host supplies the already authenticated tenant/actor and a mandatory
`EnvironmentAdmission` implementation on the original transaction. Native Veyndo
retains current `developer.manage`, signed database scope, private environment
ownership and the original personal identity or integration-credential binding.
None of these grants can be selected through a request body.

Save retains strict shared Manifest decoding, original error order, 64 KiB JSON
budget, per-actor advisory fence, row revision and maximum 20 drafts. Unfinished
packages can be autosaved without passing install-time compilation; loading a
draft never installs or approves it. Submitted locale values and omitted fields
are retained without rewriting them into a canonical published package.
The caller owns commit/rollback. No identity service or production schema host
is introduced by this library.

After the disposable PostgreSQL contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example private_drafts
```

[The example](examples/private_drafts.rs) saves and reads a private draft, denies
a stale save and unadmitted environment, demonstrates actor isolation and scoped
deletion, then rolls back. Its synthetic identities are not a production login
adapter. Automated storage regressions also cover concurrent create/update and
quota contention, database reconnect, rollback, byte-budget boundaries, denied
host checks, revision exhaustion and absence of package/outbox effects.

## App record history and restoration

Release 0.21.0 adds `record_history`: immutable scoped field snapshots with original
list-action read scopes, actor/source attribution and the existing retention
limits of 20 revisions per record and 16 MiB per app. One fixed history table
serves all apps and tenants; no installation creates extra SQL objects.

`ReadAuthority` supplies current authenticated rights. Historical read scopes are
retained even after a package changes; malformed retained scope values fail
closed. `list` returns trusted host rows after that check. The host must still
admit the current entity and project its current declared fields before output.
Raw snapshot fields are not a public DTO or permission grant.

`RestoreHost` has no default writer. It must call the normal current-package,
typed-field/reference/lifecycle/CAS write path on the original transaction. A
restoration creates a new revision and its normal history/event, preserving
existing clocks, package identity and publication rules. Native Veyndo uses its
actual managed writer. The caller owns current entity rights, explicit review,
expected revision, signed scope, actor/source context and commit/rollback.

After the disposable PostgreSQL contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example record_history
```

[The example](examples/record_history.rs) captures scoped history, denies absent
read rights and proves there is no implicit restore writer, then rolls back.
Automated PostgreSQL regressions use the actual shared CAS writer/history/outbox
to test successful restoration, two competing restores, stale versions, changed
rights/field shapes, history/audit failures and atomic rollback. Retention tests
exercise actual `pg_column_size` beyond 16 MiB and preserve other apps/tenants.
Production identity/schema hosting and final product deployment remain separate.

## Package activation and deactivation

Release 0.22.0 adds `activation`: the original state-change transaction with
mandatory host configuration/activation/deactivation hooks, original tenant/package
fences, active-dependant check and revision CAS. It uses the same lifecycle locks
as installation and active-package writers. Data, versions, credentials and jobs
retain their separate lifecycles; this operation deletes none of them and emits
no new event. Same-state requests still advance the revision, as in native Veyndo.

The trusted host admits the exact current actor and scope before calling `set`.
`StateHooks::prepare` locks the actual configuration/automation owner first.
`activating` must validate the actual installed package, current actor scopes,
consent/trust, dependencies, commerce hooks and native lifecycle. `deactivating`
refuses connected host frontends or equivalent host dependencies. All hooks are
required, with no approving defaults, on the original caller transaction.
Native Veyndo implements them through the original owners. Caller rollback is
mandatory on any hook/storage/conflict error; the library never commits.

Revision validation retains its original position after branch admission; missing
packages, frontends and dependencies keep their existing error order. An exhausted
bigint revision fails in PostgreSQL without partial state or wrapped clocks.
The standalone service identity/schema/API/MCP host remains separate work.

After the disposable PostgreSQL contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example package_state
```

[The example](examples/package_state.rs) explicitly denies both operations without
actual host authority and rolls back. Storage tests use durable synthetic host
facts/connected references for successful transitions and denial cases, then the
actual shared installer/package writer to test dependency races in both orders.
Those fixture facts do not replace production authentication or native trust.

## Schema migration and recovery snapshots

Release 0.23.0 adds `schema_upgrade`, the original bounded field migration and
recovery capture used inside package installation. Rename, remove, exact type
conversion and missing-value fill use the shared evolution model. Historical
snapshots retain the old manifest and declared fields, IDs and revisions before
conversion. The existing fixed tables serve every tenant/app/model.

Call this only inside the complete admitted installer under its tenant/package
fences. Required `Validation::fields` checks each converted record on the same
connection; the native host retains typed values, current content languages,
currency and rich-content policy. Installation still performs actual reference,
projection, consent, trust and dependency admission and owns commit/rollback.
There is no default validator, new pool, schema creation or automatic schema rewind.

The original limits remain 2,000 records per affected model, 4,000,000 serialized
bytes per recovery snapshot and 16,000,000 retained bytes per tenant/app. History
is never silently deleted. Every affected row advances its revision; an exhausted
revision returns a conflict after field validation, without wrapping or panicking.
The caller must roll back every error, including later installation failures.

After the disposable PostgreSQL contract test, run:

```sh
APP_STORAGE_TEST_DATABASE_URL=postgres://.../disposable_storage_test \
  cargo run --locked --example schema_upgrade
```

[The example](examples/schema_upgrade.rs) converts a synthetic string quantity to
an integer, reads its original recovery value and rolls back. Tests compose the
actual shared installer with required synthetic host hooks and cover multilingual
values, tenant/app isolation, conversion/validation/late failure rollback, row and
byte budgets, immutable history collisions and revision exhaustion. These fixtures
do not provide production identity or signed database authority.

## Supervise existing App jobs independently

The [App job supervisor](data-service/jobs.md) exposes bounded history/detail and
approved cancel/retry/resolve/archive through HTTP, MCP and the unchanged SDK.
An explicit fixed-schema profile keeps current identity/rights, immutable job
inputs/results, retained worker leases and atomic control events. Native Veyndo
uses the same result-redaction and queued-signal owner. Worker execution remains
with its actual producer/consumer host.

## Conversation views

The shared model validates `admin.assistant` and `storefront.assistant`
placements against a versioned native text, table or card view. The placement
requires one explicitly MCP-enabled list action with matching public or private
audience. Veyndo Core hosts the MCP App resource, checks current actor rights and
embeds the result inside the assistant response. See the
[package example and host integration](https://github.com/veyndo/veyndo/blob/main/docs/agentic-component-ui.md#app-owned-views-in-the-conversation).
