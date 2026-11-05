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
cargo test --locked -p veyndo-app-model
cargo clippy --workspace --locked --all-targets -- -D warnings
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
