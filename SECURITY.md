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
