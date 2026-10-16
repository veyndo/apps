# src/apps ownership

Root AGENTS.md also applies. Rules below were moved from the root, not weakened.

## Extension platform v2 ownership

Read `docs/app-platform.md`, `docs/app-security.md` and `docs/app-studio.md` before
changing apps. `src/apps/` owns canonical manifest validation/signing, explicit
consent, scoped callback identities, surface grants, tenant-specific logical schemas and
migrations, secrets, leased jobs and event delivery. Native app assets call the
existing `src/assets/` owner; exports use the same private asset path. Never add
parallel SQL stores, upload owners or direct core-API bypasses.

External credentials must bind to canonical approved package digests; remote UI
bytes need operator hash pins. Publication must not bypass installation consent,
current creator permissions, dependency checks, row/byte quotas or forced RLS.
`crates/apps/src/sandbox_engine.rs` is the shared Wasm engine. Prepare/probe bounded modules
before commerce transactions; WIT hooks consume bounded read-only snapshots and
use the existing native money/tax calculation owner.

Visual edits and coding-agent output share the same Manifest/AST. F5 is actor-private
and expires; it cannot call services, change live core data or release packages.
Draft CAS conflicts remain visible and do not overwrite another editor. Interface
text uses existing typed vocabularies; dynamic content uses one selected language.
New runtime routes/fields must remain aligned across API/MCP/Flow/Studio and get
real own/foreign-tenant regression checks. Never describe iframe CSP as total
hostile-code containment or local worker limits as fleet-wide admission.

### App ontology ownership

Optional `intelligence.ontology` belongs to the canonical native app manifest.
`src/apps/ontology.rs` projects only selected fields/current revisions from the
existing permission- and tenant-scoped native list. API, MCP and planner context
consume this same view; App Studio and coding agents edit the same schema.
Namespace app-owned node/edge types. Keep graph labels in `LocalizedField` and the
shared content-language inheritance system; ship UI vocabulary in EN/DE/FR/ES.
Never create another app knowledge database/ledger, bypass native reference FKs,
or treat these records as confirmed product claims. Source admission and merchant
fact confirmation still belong to the existing product claim compiler.

### App patterns belong to plugins

First-party domain app patterns live in `extensions/apps/<app>/manifest.json`,
including `returns-workspace`, `product-advisor` and `erp-handoff`.
The App Studio recipe picker only clones these canonical packages and uses the
existing Manifest compiler. Keep app-owned models, UI, AST and event declarations
in their package; never reimplement returns business logic in `src/` or the picker.
The Rust core owns shared admission, tenant storage, reference validation and the
bounded runtime. Install/review patterns in an actor-private preview or staging
before an explicit release. Returns records are private, order-bound app data;
they do not authorize refunds, change order/payment states or bypass Payments.

### All plugin/app files remain under their package owner

Every plugin keeps its implementation, native Manifest/AST, UI, styles, translations,
assets, templates and app-local tests in `extensions/apps/<app>/` (or its independent
private repository). Installation/release registers immutable packages and app data;
it never copies source into `src/`, `frontend/src/`, shared system folders or core
migrations. The same rule applies to AI-generated apps. Database schema belongs to
the existing app migration contract, not a second manually installed core schema.

Host routing/registration, generic runtime/rendering, authorization, callbacks and
shared SDKs may reference app-owned packages; they must not own app business logic.
Shared connector hosting belongs to `extensions/services/connectors/src/`; Email,
Google Analytics, Gmail and Slack implementations stay in their app roots. The
connector entry binary is separate from the commerce library. Plugin frontend modules
receive the same architecture, localization and coverage gates as the host frontend.
`scripts/testing/plugin_ownership.py` rejects known leakage paths and unsafe pattern
access. Folder containment does not itself prove runtime isolation: declarative
apps use the bounded host/F5 runtime, network apps use the existing service gateway.
The legacy PayPal compatibility adapter remains host-native for old configured
payment attempts; it is app-owned and must never be called sandboxed. New external
providers use the generic remote contract. Removing that compatibility path requires
an explicit existing-attempt/provider migration, not a cosmetic file move.


## Structured configuration ownership

Read `docs/furniture-configurator.md` before extending configuration. App packages
own library schema, dimension rules and frozen cases. Generic configuration modules
reuse field validation, no-import bounded guests, native currencies, cart/channel
admission and exact B2B approval. Saved lists store inputs only; completed orders
retain accepted package/material snapshots. Never trust a caller-authored fee or
restore an old list price. HTTP/UI and declared MCP configure actions share the
same owner; MCP opt-out does not disable HTTP. Configure is not a Flow action.
Minimized `app.configuration.changed` events use the existing atomic outbox and
Flow consumer without cart tokens or free input text. Purchase projections require
current orders.read independently of Manifest metadata and filter by app before
pagination. No furniture rule, second pricing store or app-specific SQL table in Core.
