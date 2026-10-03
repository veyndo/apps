# src/apps

This folder owns the Rust modules listed below. Each source begins with its responsibility contract. The crate currently shares internal types/imports through a facade; APIs, MCP and UCP delegate to shared domain operations.

- [`cart_contributions.rs](cart_contributions.rs): Generic app contributions: configure a cart, bind package/data revisions and persist audited pricing inputs.
- [`compatibility.rs](compatibility.rs): Explicit read adapter for persisted v0.5 engraving carts; completed order snapshots remain unchanged.
- [`data.rs](data.rs): Managed app tables: typed writes, optimistic revisions, bounded reads and local RLS context.
- [`events.rs](events.rs): Durable at-least-once app events, retry leases and stable event idempotency keys.
- [`evidence.rs](evidence.rs): Private provenance-bearing app exports feed merchant retrieval and durable app events; never public PDP answers.
- [`evidence_routes.rs](evidence_routes.rs): Scoped merchant-only evidence retrieval; sources never enter public product answers.
- [`gateway.rs](gateway.rs): One permission-aware action gateway serves HTTP, UI and MCP; service egress is operator configured.
- [`manifest.rs](manifest.rs): Strict package contract; identifiers and limits are checked before any schema DDL.
- [`manifest_validation.rs](manifest_validation.rs): Package capability, schema and action validation; no executable behavior is inferred from names.
- [`mod.rs](mod.rs): Versioned app packages: managed data, UI slots, agent tools and isolated service calls.
- [`planning.rs](planning.rs): Registered managed app actions join the same preview/approve transaction as core changes.
- [`registry.rs](registry.rs): Atomic installation and additive schema upgrades; immutable version digests preserve history.
- [`routes.rs](routes.rs): Tenant-scoped package lifecycle, generated data endpoints and a shared action adapter.
- [`runtime.rs](runtime.rs): Generic pure-Wasm contribution executor; the installed package supplies all business predicates.
- [`service_limits.rs](service_limits.rs): Non-queuing per-process bulkheads isolate slow apps without holding database connections.
- [`surface_tests.rs](surface_tests.rs): Contract counterexamples reject cross-scope UI actions, unsafe URLs and mutating GET routes.
- [`surfaces.rs](surfaces.rs): App-owned UI surfaces and namespaced HTTP routes reuse the authorized action gateway.

The [source inventory](../../docs/module-inventory.md) is checked in CI. [The behavioral map](../../docs/source-map.md) identifies integration suites, and [testing](../../docs/testing.md) describes actual coverage and limits. Every file is limited to 320 lines; `main.rs` to 120.
