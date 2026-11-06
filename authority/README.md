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
  the service consuming this context. Optional `personal_session` and `live`
  claims default to false; the identity owner must never assert them for machine,
  callback or delegated agent tokens. Settings require both plus current administrator
  scopes. They are not accepted from request bodies.
* `vault`: the original platform AES-256-GCM nonce/ciphertext Base64 envelope,
  authenticating the exact host binding. The caller supplies key bytes; this module
  reads no environment, resolves no identities and grants no database rights.

`sql/signed_scope.sql` is the exact common verifier prefix from native migration
099 at Core fd370580d97b7133272ba96fb9d31e70a357d7f7. Veyndo checks that exact
prefix during component-source verification. Historical native migrations are
unchanged. The independent data reader attaches its own fixed table policies;
it does not copy Core identity or order tables.
