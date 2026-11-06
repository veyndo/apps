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
process defaults to a restricted nonowner reader. Schemas always require the
separate migration owner. Ordinary profiles cannot install packages; an explicit
portable installation profile admits only fully compiled private app_ data packages
with current personal/live administrator consent, package CAS, shared installer/model
sequence, immutable versions and fixed row/byte budgets. An explicitly enabled managed-write profile uses only admitted portable
private records with package/revision fences, shared fields/workflow/history and
atomic outbox. Native resource-dependent models require their actual policies. Native object-bound models require the actual resolver; deny them until
that port is implemented. authority/ holds the original shared signing protocol
and fresh fixed-endpoint introspection, with opt-in network/database features.

The optional data-service settings profile requires its explicit schema/grant
upgrade and a separate operator vault key. Current personal/live claims must come
from the fixed identity owner, never App/request fields or delegated tokens.
Native settings use the same shared encrypted partial-update sequence; native
verified personal/live administration remains attached. Vault provides the
original envelope only, without key loading, SQL or identity authority.

The optional schema-5 job supervisor admits only history/detail and original
control operations. Preserve exact consent/CAS/current action input and rights,
redact stale/revoked results through job_projection, and never grant enqueue,
result/input/message or schema writes to its serving login. Trusted producers,
worker callback/evidence and actual dispatch remain separate owners. No fake
completion, automatic unknown retry, second outbox consumer or public seed path.
