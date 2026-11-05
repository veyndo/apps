# Pure model ownership

Keep this package independent of SQL, network, Core, Wasm runtime and identity
resolution. Preserve native wire serialization, missing/null language semantics,
closed action-schema grammar, resource budgets, workflow and publication rules.
Required policy ports delegate canonical currency/content/reference checks to the
host. Never add permissive defaults or infer grants from model metadata.

Core consumes these same types/functions. Test unknown fields, exact values,
unsafe schemas, escaped pointers, language inheritance and rejected transitions.
Every source module has a responsibility comment and at most 320 lines.
