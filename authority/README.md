# Shared App host authority

`veyndo-app-authority` shares Veyndo's original role-bound database ticket protocol
and the fixed current OAuth introspection adapter between trusted composition roots.
It creates no pools, owns no users or roles, issues no user tokens and reads no
environment variables. Enable only the adapters your host needs:

* `database` (default): exact native `Scope`, `Signer`, safe identity grammar and
  `TenantSigner`. Core retains its task-local scope, pool and provisioning owner.
  A tenant-only reader exposes actor tickets and has no system-ticket operation.
* `oauth`: immutable fixed HTTPS configuration and fresh bounded RFC 7662 requests.
  No redirects/proxy forwarding, no grant cache, bounded replies and current
  resource audience/expiry/tenant/subject validation. Domain rights remain with
  the service consuming this context.

`sql/signed_scope.sql` is the exact common verifier prefix from native migration
099 at Core fd370580d97b7133272ba96fb9d31e70a357d7f7. Veyndo checks that exact
prefix during component-source verification. Historical native migrations are
unchanged. The independent data reader attaches its own fixed table policies;
it does not copy Core identity or order tables.
