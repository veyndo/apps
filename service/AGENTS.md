# Standalone runtime service

This is the network composition root, separate from src/ storage, model/ and the
no-network runtime/ library. It may configure the trusted identity HTTP adapter,
bind the server and call the same bounded runtime. Never add network, pool or
identity configuration to the library owners.

Every protected request requires fresh token introspection at the fixed operator
endpoint; request roles/tenants/scopes are never authority. Keep response/error
redaction, strict input/output contracts, bounded admission and disconnect-safe
execution permits. Evaluation uses supplied synthetic snapshots only and has no
SQL, live record fetch, write, provider, key issuance or payment capability.
HTTP and MCP must dispatch the same operation. Run real process HTTP/MCP/identity
fixtures and JS client tests without Core, database or paid provider calls.
