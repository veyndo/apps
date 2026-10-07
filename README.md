# src/apps

This folder owns the Rust modules listed below. Each source begins with its responsibility contract. The crate currently shares internal types/imports through a facade; APIs, MCP and UCP delegate to shared domain operations.

- [`cart_contributions.rs`](cart_contributions.rs): Generic app contributions: configure a cart, bind package/data revisions and persist audited pricing inputs.
- [`compatibility.rs`](compatibility.rs): Explicit read adapter for persisted v0.5 engraving carts; completed order snapshots remain unchanged.
- [`data.rs`](data.rs): Managed app tables: typed writes, optimistic revisions, bounded reads and local RLS context.
- [`editor_contract.rs`](editor_contract.rs): Editor mounts, enumerated fields and tenant-owned core references; no app alters core tables.
- [`editor_tests.rs`](editor_tests.rs): Assistant fixture contracts exercise real validation, not only the client-side builder.
- [`events.rs`](events.rs): Durable at-least-once app events, retry leases and stable event idempotency keys.
- [`evidence.rs`](evidence.rs): Private provenance-bearing app exports feed merchant retrieval and durable app events; never public PDP answers.
- [`evidence_routes.rs`](evidence_routes.rs): Scoped merchant-only evidence retrieval; sources never enter public product answers.
- [`gateway.rs`](gateway.rs): One permission-aware action gateway serves HTTP, UI and MCP; service egress is operator configured.
- [`service_policy.rs`](service_policy.rs): Exact operator-owned transport admission; the extracted predicate rejects unsafe URLs and unapproved HTTP origins.
- [`manifest.rs`](manifest.rs): Strict package contract; identifiers and limits are checked before any schema DDL.
- [`manifest_validation.rs`](manifest_validation.rs): Package capability, schema and action validation; no executable behavior is inferred from names.
- [`mod.rs`](mod.rs): Versioned app packages: managed data, UI slots, agent tools and isolated service calls.
- [`native_data.rs`](native_data.rs): App record translations use configured shop languages and field-level main-language inheritance.
- [`native_view_tests.rs`](native_view_tests.rs): Native schema security regressions: bindings, public writes, allowlists, bounded blocks and legacy digests.
- [`native_views.rs`](native_views.rs): Bounded native view definitions; every data binding resolves to the same authorized app action gateway.
- [`planning.rs`](planning.rs): Registered managed app actions join the same preview/approve transaction as core changes.
- [`presentation.rs`](presentation.rs): Optional passive app artwork and localized summaries; omitted metadata preserves published legacy digests.
- [`registry.rs`](registry.rs): Atomic installation and additive schema upgrades; immutable version digests preserve history.
- [`routes.rs`](routes.rs): Tenant-scoped package lifecycle, generated data endpoints and a shared action adapter.
- [`runtime.rs`](runtime.rs): Generic pure-Wasm contribution executor; the installed package supplies all business predicates.
- [`schedules.rs`](schedules.rs): Durable UTC cron ticks emit namespaced outbox events; replicas lock due rows and staging never runs them.
- [`service_limits.rs`](service_limits.rs): Non-queuing per-process bulkheads isolate slow apps without holding database connections.
- [`surface_tests.rs`](surface_tests.rs): Contract counterexamples reject cross-scope UI actions, unsafe URLs and mutating GET routes.
- [`surfaces.rs`](surfaces.rs): App-owned UI surfaces and namespaced HTTP routes reuse the authorized action gateway.
- [`webhooks.rs`](webhooks.rs): Operator-signed incoming events: tenant-bound HMAC, five-minute freshness and atomic replay receipts.

The [source inventory](../../docs/module-inventory.md) is checked in CI. [The behavioral map](../../docs/source-map.md) identifies integration suites, and [testing](../../docs/testing.md) describes actual coverage and limits. Every file is limited to 320 lines; `main.rs` to 120.

- `editor_contract.rs`, `editor_tests.rs`: owned core references, context/choice/privacy validation and direct-route scope checks.
- `schedules.rs`: transactional UTC emit ticks, persistent clock receipts and private-stage exclusion.
- `webhooks.rs`: exact-byte HMAC verification, typed ingress and durable replay receipts.
- `native_views.rs`, `native_data.rs`, `native_view_tests.rs`: bounded native binding contracts, action handlers and validation cases.
- `presentation.rs`: declared artwork and local category fallbacks.

[Guided app contracts](../../docs/app-assistants.md).
