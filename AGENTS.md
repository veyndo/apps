# App storage ownership

Keep storage SQL in this repository and preserve its provenance. Veyndo imports
an exact source snapshot; changes belong here. The public API accepts an already
admitted host connection/scope. Do not invent principals, bypass native validators,
open an environment-configured pool or introduce provider/network dispatch.

Preserve tenant/app/entity keys, caller transactions, job request fingerprints,
package/action binding, quotas, atomic outbox, lease fences and uncertainty.
No per-app/model SQL objects and no second commerce ledger. Native field rules,
signed RLS, current authority, history and publication stay with their host owners.

Run formatting, lint and the ignored real PostgreSQL contract test explicitly.
Each Rust module starts with its responsibility and stays below 320 lines.
Integration with the native product requires its affected app/storage/job suites.
