# Security

Report vulnerabilities privately to the repository maintainer before publishing
customer data, credentials or exploitation details. Never include real access
keys, database URLs, sessions or order data in issues or fixtures.

The library is for trusted hosts. A storage Scope is not authorization. Callers
must retain current actor/package/reference checks and a restricted scoped
connection. Schema installation and signed row security remain host duties.

Uncertain external effects must be reconciled before retry. Expired jobs are not
automatically reclaimed. Security fixes are maintained on main and identified in
component releases.

## Independent read host

`data-service/` accepts private installed list actions only, through fresh current
OAuth merchant authority. A separate nonowner read-only PostgreSQL login, signed
role-bound context and forced RLS are mandatory. The serving process neither runs
schema installation nor exposes a seed/write endpoint. The operator's migration
and installation authority remains separate. See its README for the explicit
schema/key setup and the still-required native object resolver.

The opt-in managed-write profile is a separate schema/permission upgrade. It
requires current apps.manage and installed save rights, explicit confirmation,
package digest and record revision. It shares the original mutation/state/history
and atomic outbox sequence. Unsupported native policies deny. See the managed-write
guide for exact positive/forbidden database privileges and uncertain-response handling.

## Optional settings profile

Schema 3 is an explicit migration-owner upgrade of schema 2, never a startup
migration. A separate fixed secrets table admits only `settings`, with forced
signed tenant RLS and bounded authenticated ciphertext. No per-App SQL objects.
The service needs `APP_DATA_MODE=write`, `APP_DATA_SETTINGS=enabled` and its own
private key. Both scopes and nondelegatable `personal_session: true`, `live: true`
claims must come from fresh fixed-endpoint introspection. Requests cannot supply
these claims. Every settings read/write requires apps.read, apps.manage and
team.manage. No secret export/resolution endpoint exists.

The trusted installation owner registers each package's canonical digest.
Settings updates require explicit approval/current package and revision; removed
settings and previous package secrets never inherit across upgrades. Partial
updates preserve omitted current secrets. Wrong key/binding, late event failure
or CAS conflict rolls back without emitting values. Vault keys are not scope
signing keys. Their safe storage, backup and coordinated rotation belong to the
operator; schema migration does not rotate encryption keys.

## Explicit portable installation profile

Schema 4 adds narrow package/model DML, immutable version inserts and protected
per-App budget accounting. No owner, DDL, signing-key, version-update/delete or
quota-counter update authority is granted. Exact profile startup checks remain
mandatory. SQL policies keep reserved package definitions outside installer writes.
The operator supplies the scope catalogue; packages never grant their own scopes.
The complete compiler and portable profile deny native references, publisher trust,
UI, external effects and explicit data migrations until their real policies exist.
Review is read-only; install requires a fresh personal live administrator, exact
permissions/digest and current package revision. Models, package/version, budget
initialization and minimized event commit together. No preview/publication flag
can be supplied to skip immutable version admission.

The explicit job-supervision profile adds one fixed forced-RLS table. Its login
cannot enqueue, change input/result/message/keys or create SQL objects. Current
OAuth supplies identity; personal live administrative authority, explicit digest/
revision approval and reviewed uncertain outcomes gate control. Original leases,
CAS and atomic events remain shared. Historical results require current package
and all action rights; worker tokens and inputs never enter responses.
