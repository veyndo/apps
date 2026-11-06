# App model contracts

Pure Rust model contracts extracted from Veyndo's native app owner. This package
has no SQL, network, identity resolver or Veyndo application dependency.

- `Action`, `Entity`, `Field`, `Choice` and extension types preserve exact native wire defaults.
- `declarations::validate` checks model/field identifiers, relationships, choice
  labels, index budgets, publication and workflow declarations.
- `schema` owns the closed action-input grammar and bounded recursive evaluation.
  Transport-neutral errors retain bad-input/conflict/validation and JSON pointers.
- `fields::validate` checks exact types, choices, additional constraints and
  translated values through the required `ValuePolicy` host adapter.
- `language::validate` checks enabled languages and required main-language values.
- `lifecycle::change` enforces initial states, permitted edges and immutable fields.

A host must implement `ValuePolicy` and `ReferencePolicy` using its authoritative
currency, rich-content, relation-shape and reference rules. There are no permissive
default implementations. Veyndo delegates to its existing money/content owners and
verified core-reference predicate. These pure policies do not replace current
identity, installed-package admission, reference ownership, database isolation,
CAS, history or transaction checks. Plugin input cannot choose the host policy.

Use `schema::validate` for bounded app actions; the explicitly separate
`validate_native` budget exists for trusted native rich-product requests.
Unsupported schema keywords such as external `$ref` are rejected at admission.
The supported grammar is intentionally closed rather than a full JSON Schema engine.

Run from this repository root:

```sh
cargo test --locked --manifest-path model/Cargo.toml
cargo clippy --locked --manifest-path model/Cargo.toml --all-targets -- -D warnings
```

The native host retains its existing HTTP/MCP/Flow routes and calls this package.
A separately authenticated standalone app service is still on the extraction list.

`admission::check` requires a host `Authority` to resolve the merchant and current
scopes. `additional` and `requested_scopes` require the narrower `ScopeAuthority`.
There are no default grants. Action definitions must come from a fully validated,
consented package, not the action request body. Public-action classification,
MCP exposure and callback key issuance retain their full package/host checks.
`callback_facts` supplies independent consent/role facts to the host's existing
verified predicate; it does not authenticate a caller or approve a callback alone.

## Package compiler and host extensions

`package::compile` owns the original common package grammar: identity, runtime,
version and budgets; capability declarations; model/index constraints; configuration
bindings; action input/output schemas, privacy/Flow/job/payment/asset rules; legacy
slots and event names. It requires a trusted `Policy` for the current scope
registry, Wasm kind inspection and native reference rules. There are no default
policies. This is declaration validation; actual identity/current-rights admission
still runs separately on every operation.

The `Contract` is a borrowed projection from the host's complete typed package. It
is deliberately not a replacement JSON decoder or proof of provider approval. The
required `Extensions::before` and `after` methods must validate every remaining
feature from that same complete package. Veyndo retains all original validators
for lifecycle, settings, UI, distribution/signing, payments, hooks, schedules and
other domain extensions, in their original order. No plugin/request may choose
these trusted host implementations. An independent host must supply its own full
decoder and domain validators before claiming complete package support.

`model/tests/package_contract.rs` and `package_actions.rs` exercise this interface
without Veyndo, SQL, network or Wasm execution. The twelve added tests include
required extension denial/order, package budgets, schema closure, append-only
forms, private/public reads, jobs, payment actions, assets and configuration/Flow
restrictions. `Slot` retains the native strict wire type and published digests.

## Settings and explicit model upgrades

`settings::Setting` retains the strict native wire shape and localized labels.
`settings::prepare` applies a bounded partial update after the host admits the
current actor, package digest and revision. Required values, exact integer/boolean
values, string/control limits and the total encoded size are enforced. Pass its
encoded result only to the encrypted store. `visible` omits secrets, `configured`
returns only declared key names and `declared` drops removed settings before
runtime dispatch. These helpers cannot grant access or encrypt values themselves.

`evolution::{Migration, Step}` retains explicit rename/remove/convert/fill steps,
reserved-field and path budgets. `shape` plans changes to existing entity contracts;
operational-to-configuration changes, extension rebinding, relationships and model
removal retain the native offline-migration requirements. `convert` uses exact
integer/string conversions. The host still validates the complete package and
migration path, takes locks and recovery snapshots, validates every converted field
and reference, and rolls back every record/package/event write on failure.

Run an actual independent example with no credentials or database:

```sh
cargo run --locked --example upgrade_settings
```

This example prepares a settings update and model plan. Applying it to stored data
requires the host transaction and its current authority checks.

## Events, schedules and incoming webhooks

`events::Context` is a borrowed projection from the host's complete, currently
admitted package. `subscriptions` checks declared event permissions, own-app record
binding, B2B read rights and the original field minimization rules. Filters use
that projected payload rather than raw events, so hidden customer fields cannot
become delivery signals. Declaration permissions do not establish current actor
rights; the host must still reauthorize the installed package before dispatch.

`events::validate` requires a `DestinationPolicy` with no default acceptor.
Veyndo supplies its existing HTTPS/public-address validator. External hosts must
supply their own URL and destination admission, then independently enforce DNS
resolution, pinned network access, signatures and current package authority at
actual dispatch. This pure module never opens a socket or creates a delivery queue.

`triggers::{AppSchedule, AppWebhook}` retains strict native wire shapes. UTC cron
parsing permits one seconds value and at most eight schedules; validation accepts
an explicit host clock and checks declared emit actions and their bounded inputs.
Incoming webhook declarations require private emit actions. Actual HMAC checks,
freshness, replay receipts, tenant binding, durable ticks, leases, private-environment
restrictions and all transactional effects remain host responsibilities.

```sh
cargo run --locked --example event_contracts
```

The example evaluates a minimized event and a next UTC tick without Core, SQL,
credentials or network. Its destination policy rejects every external URL.

The model keeps its own lockfile for reproducible standalone builds. CI invokes
its manifest directly, so model tests and lint run as the root package rather
than relying on dependency lint behavior in the storage build.

## Native object bindings and graph views

`object_extensions::validate` checks app-owned model bindings to products,
customers, orders and companies. A binding requires the original private unique
reference anchor, declared list/data read and native object scopes on every
list/save action. PII requirements retain the exact object-specific scope sets.
Current actor permission checks, referenced-object ownership and the verified
configuration-release predicate stay with the host.

`intelligence::Contract` is the strict tool/entity metadata used by the native
package. Its validator checks declared tools/models and existing budgets; it does
not opt an action into MCP or admit an agent request. `ontology::Mapping` validates
selected fields, namespaced types and declared reference edges. `ontology::project`
derives a bounded graph view from already-authorized current rows with source
app/version/record/revision and selected properties. Edges identify references;
they do not resolve target records, grant reads or confirm product claims.

A host must keep its full typed package compiler, current action/entity rights,
tenant-scoped queries, field/language/reference validation, private/public release
and projection admission connected. Supplying an app/tenant string is not proof
of identity. The pure projector owns no database, graph store or inference client.

```sh
cargo run --locked --example object_graph
```

The independent example validates a product extension and prints its graph from
synthetic admitted rows. A real host must obtain the input rows through its current
permission-aware list. Unknown fields retain strict rejection; optional mappings
keep legacy omission behavior.

## Placements and named HTTP routes

`surfaces::{Surface, ApiRoute}` retains strict native wire types and legacy
serialization. `surfaces::validate` checks placement locations/IDs/paths, declared
private/public actions and slot permissions, route scope/method/name uniqueness
and the original resource budgets. Native validation errors retain exact JSON
pointers, including unavailable action and UI-path errors.

A trusted host must implement `Policy` for existing native views, known team
scopes and read-only effects. There are no default approvals. Veyndo delegates to
its original view registry, current scope catalog and verified read predicate;
a caller's `readOnly` flag cannot make save/emit/submit safe for GET. The complete
native view/behavior compiler remains separately mandatory after this contract.

Actual actor/context-bound expiring surface grants, current installed-package
checks, pinned UI bytes/CSP, HTTP dispatch, action input/output admission and
MCP opt-in remain host responsibilities. A placement or route declaration grants
no data access and starts no service. Root `Contract` is only a projection from
the complete typed package and does not replace its other compiler stages.

```sh
cargo run --locked --example surface_routes
```

The example uses explicitly registered view names/scopes and a conservative named
read action. It validates declarations without serving a UI or implementing the
full view compiler, identity or runtime. A real host must supply those owners.


## Native views and UI behavior

`views::Contract` borrows entities, actions, surfaces and native views from the
complete typed package. `views::validate` checks exact localized view/control
wire types, twelve-column geometry, editor context bindings, action/model
bindings, container ownership and bounded click/change behavior. UI statements
support typed assignment, comparisons, action calls, messages, navigation,
refresh and form validation. Unknown properties and prototype/path object keys
are rejected. Action-binding errors preserve their exact JSON pointers.

The required `views::Policy: fields::ValuePolicy` supplies the host's actual field
value policies and public-read decision. Veyndo uses its existing currency,
rich-content, relation and language owners and the original verified public-read
predicate. There are no default approvals. A standalone host must implement these
ports from its own canonical owners; accepting a declaration never creates a user
identity, runtime grant or permission to execute its actions.

`views::payload` projects the selected view, bound entity definitions, allowlisted
lookup/asset actions and navigation within the same public/admin class. Call it
only after current host authorization. It neither reads records nor renders or
executes behavior. The existing native renderer, interpreter and action gateway
consume the same wire model after their normal admission checks.

Run `cargo run --locked --example native_views` from the repository root for a
localized form and its validate/save behavior, without a Core binary or database.
Independent compiler cases are in `model/tests/native_views.rs`; the original
care-studio contract fixture is retained with extraction provenance.

## Configuration and executable business contracts

`configuration` owns the exact configuration DTO, structured fields, frozen cases,
public library/lookup declarations, record selection and exact integer/boolean
arguments. Required `Policy` ports reuse canonical currency, field-value and core
reference rules. Legacy text configuration retains character counts and strict
single-field input. This contract does not fetch a library record or select a
current installed package.

`commerce_hooks` shares strict source/hook/read-only-record declarations. Its
source-kind callback must use the trusted bounded Wasm executor; permissions and
record declarations remain required. `business_capability` and `business_snapshot`
share localized intent, bounded synthetic snapshots, declared hook bindings and
expected decisions with exact money and error pointers. Frozen records receive
the same host field validation; no live data is read.

The model stays pure and does not depend on Wasmtime, SQL or network clients.
The runtime package projects admitted frozen snapshots into WIT, executes the
actual guest and compares its result. Model declaration validation and executed
example acceptance are separate mandatory steps in the complete native compiler.
See the [independent business preflight](../runtime/examples/business_preflight.rs).

## Native order and quote lifecycle contracts

Apps 0.14.0/model 0.10.0 shares `native_lifecycle` declarations, extended graph
wire types, bounded compilation, required-main-language labels and narrowing facts.
A trusted `CompilerPolicy` supplies the admitted native order graph, known current
scope catalogue and locale grammar. `Package` borrows the complete admitted
package ID/version/digest. Compute that digest with the host's full canonical
Manifest, never the lifecycle projection. The SHA-256 state/action namespace stays
identical to the original host.

`validate` admits one order and one quote declaration, then compiles each. For
combined installation, call `compile` with all active admitted packages and the
already-validated native base. It rejects competing replacements, stranded states,
unknown scopes/guards, terminal exits and attempts to replace financial or buyer
acceptance decisions. Native `start`/`complete` can be narrowed. The component does
not execute transitions or invent payment/delivery facts.

`native_lifecycle_snapshot` decodes the immutable accepted graph, preserves its
revision and projects customer-visible labels without policies. Malformed or
wrong-object/version snapshots fail with `Conflict`; they never silently fall
back to the current graph. `reason` calls a required current-permission callback
and checks declared facts. It is eligibility narrowing, not authorization to apply
an order transition. The host retains actual identity/tenant checks, native domain
validation, payment/delivery/buyer checks, transactions and final verified predicate.

```sh
cargo run --locked --example native_lifecycles
cargo test --locked --manifest-path model/Cargo.toml --test native_lifecycle
```

The executable example compiles order quality review and quote preparation and
checks accepted labels plus revoked permission. Fixtures are synthetic; no database,
Core binary, external payment or merchant credentials are used.

## Complete package wire and domain contracts

Apps 0.15.0/model 0.11.0 exports `manifest::Manifest`, the full native strict JSON
contract assembled from all shared domain types. Core aliases that exact type.
Every optional/default/omitted field retains its original serde behavior; 34
complete native package fixtures retain their canonical fingerprints. Decoding
alone does not admit a package or approve its provider.

`Manifest::common_contract` borrows the original common compiler projection.
`manifest::value_digest`, `canonical_digest` and `signature_message` share the
original sorted serde_json representation. Only the signature itself is cleared
from the signed message. The storage package delegates to this same fingerprint.
Existing package/action/version/source fences therefore use identical bytes.
Recursive key ordering preserves those fingerprints even when a downstream host
unifies serde_json with `preserve_order`; CI checks that explicit feature case.

`presentation::validate` keeps passive media/assets and credential-free HTTPS
artwork grammar. The `url` dependency only parses URLs; it never fetches them.
`payment_provider::validate_contract` keeps service/category/consent, native
reserved IDs and the closed method/currency/country/intent/capability grammar,
including multi-capture prerequisites. It does not execute or authorize payments.

`distribution::verify` runs metadata, mandatory `SignatureVerifier`, then declared
dependency validation in the original order. Implement that port with the current
operator's registered keys and full `signature_message`; native Veyndo retains
its original Ed25519 verifier and exact HTTP failure semantics. There is no default
trust implementation. `require_version` checks the verified installed publisher
and semantic version. `validate_graph` rejects cycles, more than 256 reachable
apps and upgrades breaking active dependants. Supply only current tenant-scoped,
locked active packages from the trusted host. Signature validity does not grant
service access, current user rights or installation consent.

```sh
cargo run --locked --example package_contracts
```

The example decodes all 34 full packages, signs/verifies a synthetic provider with
actual Ed25519, rejects altered permissions and a revoked operator key, and checks
passive artwork/payment declarations. No Core process, database, network or real
merchant secret is used. Full domain admission still runs through the required
native compiler/policy ports and actual host effects.

## Complete standard package compiler

Apps 0.16.0/model 0.12.0 provides `compiler::compile(&Manifest, &Policy)`.
It runs all 19 standard domain stages in their original native order: native
object bindings, order/quote lifecycles, settings, configuration, artwork,
ontology, schema evolution, publisher distribution, commerce hooks, business
examples, payment declarations, common grammar, subscriptions, event delivery,
schedules, incoming webhooks, surfaces, intelligence metadata and native views.
The first failure retains its model kind/pointer or actual host error. Domain
selection is not a caller option and there are no before/after host callbacks that
can accidentally omit a standard domain. The older `package::compile` is the
lower-level common grammar and does not replace this complete entry point.

The required policy combines existing field/reference/currency/source/scope,
native base and public-read contracts, the current operator signature verifier,
unsigned namespace and destination admission, read effects and the host UTC clock.
No policy method has an approving default. Native Veyndo delegates to its original
verified reference/read predicates, canonical currency/content policy, bounded Wasm
kind parser, current key registry and network policy. Installation consent, actual
reference ownership, module preparation/ABI verification, live identity, database
scope and effects remain outside pure package compilation.

[Independent compiler example](../runtime/examples/package_compile.rs) runs all 34
complete native packages through this same compiler with actual bounded-source
kind parsing, explicit synthetic host policies and unchanged package fingerprints.
Five runtime regressions cover all packages, one invalid input in each of the 19
stages, exact first-error/host-callback order, denied policies and actual native-view
lookup. Unsupported money/richtext/relations and all publisher signatures fail
closed in the synthetic example; it is not a production policy implementation.

```sh
cargo run --locked --manifest-path runtime/Cargo.toml --example package_compile
cargo test --locked --manifest-path runtime/Cargo.toml --test complete_compiler
```
