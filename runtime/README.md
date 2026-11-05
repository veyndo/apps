# Veyndo App Runtime

Execute real core-Wasm and WIT commerce guests without a Veyndo server,
database, HTTP client or identity service. The embedding host supplies the
reviewed module, already-authorized snapshot and required `ResourcePolicy`.

```sh
cargo run --locked --manifest-path runtime/Cargo.toml --example quote -- 25000
cargo test --locked --manifest-path runtime/Cargo.toml
```

The example executes a real read-only price hook. With a 25,000 minor-unit cart
it returns a 250 minor-unit adjustment. [Example source](examples/quote.rs).

## Native integration

The composed Veyndo product uses this same pinned source. Its small `veyndo-apps`
adapter supplies the existing verified resource predicate and operator-configured
legacy namespace list. Current merchant rights, package/digest fences, authorized
snapshot construction and applying the outcome remain with native owners. Module
compilation is prepared outside commerce locks using the existing bounded cache.
A returned adjustment is a proposal, not an order mutation or payment authority.

## API

- `sandbox::Sandbox::new(source, policy, namespaces)` compiles a no-import core-Wasm
  module or a typed commerce component. Core guests expose integer approval and
  configuration hooks. Every call gets a fresh store.
- `component_runtime::Module::new(source, policy, namespaces)` compiles the WIT
  contract in [wit/commerce.wit](wit/commerce.wit). `validate_interface` checks its
  actual ABI; `evaluate` executes price, shipping, discount or validation hooks.
- `ResourcePolicy::admitted(Resources)` is mandatory. There is no permissive default.
  Implement it from the embedding host's canonical policy owner, never package or
  HTTP request flags. The runtime also retains its own engine/store limits.
- `Snapshot` contains supplied cart, product and app-record values only. The guest
  can read that snapshot; it cannot fetch other data or access host credentials.

The core approval ABI is separate from the WIT ABI. A compiled core module may
lack an expected function; calling that hook still rejects the wrong ABI. Always
validate the actual required interface before installing a package.

## Preserved limits

Source is limited to 32 KiB; stores retain at most one 1 MiB memory and 10,000
table elements. A bounded 64-instance engine pool, limited stack, per-call fuel
and epoch deadlines remain active. Core calls use 10,000 fuel and ten 10 ms epochs;
WIT calls use 100,000 fuel and five epochs. A component can make at most 128
read-only host calls. Engine exhaustion and traps return errors.

WIT snapshots retain at most 100 cart items/products, 32 records and 64 KiB record
content; input is at most 8 KiB. Money currency/scale and output adjustments/reason
codes retain their original checks. Validation hooks cannot adjust money. Core
configuration calls accept one to eight declared integer arguments. No WASI,
network, filesystem, database or secret imports are installed.

Limits are per process/call and use Wasmtime; they are not an OS process boundary
or fleet-wide quota. Resource admission and execution do not authenticate a caller,
create a package grant, or establish a standalone Apps API/MCP service.

The WIT file here is the compiler input; the composed Core checks that its SDK
contract mirror has identical bytes. Fixtures preserve the original synthetic
examples with repository provenance. Independent CI runs the original guest
regressions, explicit host denial, fresh-store/source-identity, namespace and
snapshot guards. Native composition additionally tests actual quote/checkout,
configuration, preview, release, API/MCP, current rights and tenant isolation.
