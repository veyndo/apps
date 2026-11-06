# App storage ownership

Keep storage SQL in this repository and preserve its provenance. Veyndo imports
an exact source snapshot; changes belong here. The storage API in src/ accepts an already
admitted host connection/scope. Do not invent principals, bypass native validators,
open an environment-configured pool or introduce provider/network dispatch.

Preserve tenant/app/entity keys, caller transactions, job request fingerprints,
package/action binding, quotas, atomic outbox, lease fences and uncertainty.
No per-app/model SQL objects and no second commerce ledger. Pure model rules live in model/ and are used by the native adapter. Signed RLS,
current authority, reference ownership, currency/rich-content policies, history
and publication authority stay with their host owners.

Run formatting, lint and the ignored real PostgreSQL contract test explicitly.
Each Rust module starts with its responsibility and stays below 320 lines.
Integration with the native product requires its affected app/storage/job suites.

The independent network composition root is service/. Its required current OAuth
identity adapter may make fixed introspection requests and serve the same bounded
runtime through HTTP/MCP. It has no SQL/pool, issuer, user/role store, live entity
resolver or financial writer. Keep network configuration out of model/, runtime/
and the src/ storage library; their authority rules above remain unchanged.

The separate data-service/ root owns HTTP/MCP hosting, fixed-schema
installation tooling and current OAuth/signed PostgreSQL admission. Its serving
process defaults to a restricted nonowner reader and cannot install packages or
schemas. An explicitly enabled managed-write profile uses only admitted portable
private records with package/revision fences, shared fields/workflow/history and
atomic outbox. Native resource-dependent models require their actual policies. Native object-bound models require the actual resolver; deny them until
that port is implemented. authority/ holds the original shared signing protocol
and fresh fixed-endpoint introspection, with opt-in network/database features.
