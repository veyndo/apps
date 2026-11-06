# Supervise existing App jobs independently

Inspect and control existing long-running App jobs through HTTP, MCP and the
unchanged JavaScript SDK. The independent Rust service uses the same scoped
history, current-package/current-rights redaction and atomic control engine as
Veyndo. It runs without a Core process or access to Core tables.

## Operator setup

Start with the dedicated [App Data setup](README.md) and explicit write, settings
and installation upgrades. As their original migration owner, upgrade schema 4:

```sh
psql "$APP_DATA_MIGRATION_URL" -X -v reader_role=app_data_reader \
  -f data-service/sql/enable-jobs.sql
```

Enable `APP_DATA_JOBS=enabled` alongside `APP_DATA_MODE=write`,
`APP_DATA_SETTINGS=enabled` and `APP_DATA_INSTALLS=enabled`. Previous profiles
remain disabled for jobs and keep their earlier exact schemas/grants. Restart
with the same private identity, signing and encryption configuration. The serving
process cannot migrate the database.

The upgrade adds one fixed `app_jobs` table and one fixed history index. Every
shop and App shares their composite keys. Forced signed RLS isolates tenants.
The serving login can read jobs, change only original control columns and delete
terminal jobs. It cannot insert jobs, change input/result/message/identity fields,
create tables or acquire owner rights. Startup rejects missing/excessive grants,
an owner/bypass login or a schema/profile mismatch.

An existing trusted installation/producer owner provisions admitted worker App
packages and creates jobs through the shared library's existing enqueue contract.
The portable data installer does not install worker/service packages. The actual
producer must retain complete package validation, current actor/action/input
admission, idempotency and original active/retained job quotas. The supervisor
provides no public seed, enqueue, claim, progress or result endpoint. Worker
execution, callback evidence, provider effects and consumption of queued signals
remain with their real admitted owners. This is a supervision service, not a
second dispatcher or identity issuer.

## List and inspect

```js
await client.initialize();
const page = await client.tool('apps.jobs.list', {
  app: 'app_reports', limit: 20
});
const job = await client.tool('apps.jobs.detail', {
  app: 'app_reports', id: page.jobs[0].id
});
```

HTTP uses `POST /v1/jobs/list` and `POST /v1/jobs/detail` with the same JSON.
List supports `limit` (1 to 100), `after` and `group` (1 to 100 bytes). Ordering
uses creation time and ID. A missing/foreign cursor returns a conflict rather
than silently restarting. `group` is only a correlation label, never authority.

Fresh fixed-endpoint introspection supplies the current merchant, tenant,
subject, audience, expiry and `apps.read`. List additionally needs `apps.manage`.
Messages/results are hidden whenever the package digest or current primary and
additional action rights no longer match. Internal row package digests, inputs,
leases and worker tokens are never returned. The list's top-level `packageDigest`
is the current installed contract used for explicit control approval.

Detail requires the original actor or `apps.manage`, an active matching package,
a still-installed job action and all current action rights. It returns the
succeeded result only. Expired running/cancel-requested leases appear as
`uncertain`; they are never treated as proof that an external effect failed.

## Approve a control operation

```js
await client.tool('apps.jobs.control', {
  app: 'app_reports', id: job.id,
  packageDigest: page.packageDigest, revision: job.revision,
  operation: 'cancel', approve: true
});
```

HTTP uses `POST /v1/jobs/control`. Control requires a current personal live
administrator with `apps.read apps.manage team.manage`, exact current package
digest, positive job revision and explicit approval. Caller identity, tenant,
worker result and arbitrary event fields are rejected. All responses are
`no-store`; `/v1/contract` version 5 and OpenAPI describe the same operations.

* `cancel`: queued work becomes cancelled; running work becomes cancel-requested
  and retains its original worker lease. Cancellation is not fabricated completion.
* `retry`: failed, uncertain or expired work can be queued only after
  `approveUnknownOutcome: true`, current installed action rights and validation of
  its stored input against the current action schema. The original transaction
  resets the actor/package/progress and emits the admitted queued signal.
* `resolve_cancelled`: explicitly resolve uncertain/expired work after reviewing
  its external outcome with `approveUnknownOutcome: true`.
* `archive`: remove only succeeded, failed or cancelled jobs. Active work cannot
  be archived, including through the actual serving login's SQL row policy.

The original row lock and revision compare serialize competing controls. Job
changes and minimized outbox metadata commit together. A late failure rolls back
both updates and archival deletion. After a timeout, reload detail before
retrying; an unknown network result does not authorize another external effect.

## Executable example and acceptance

[The SDK example](examples/jobs.mjs) reads and explicitly cancels an existing
synthetic queued job. Supply `APP_DATA_URL`, a current personal `APP_DATA_TOKEN`,
`APP_DATA_SDK_MODULE`, `APP_JOB_APP` and `APP_JOB_ID`. Run it only against a job you
intend to cancel. It creates no producer, worker or financial authority.

`tests/job_cases.py` runs the actual Rust service, PostgreSQL, fixed identity
owner and unchanged SDK. It exercises two shops, own/other actors, revocation,
bounded/tied history, result redaction, cancel/lease preservation, reviewed
retry/resolve, archive, CAS races, late rollback, SQL privilege boundaries,
restricted startup and restart. The actual container repeats this acceptance.
Native Veyndo consumes the same extracted projection/event helper while retaining
its original authentication, detail/actor checks, retry admission and execution.
