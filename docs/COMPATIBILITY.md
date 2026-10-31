# Compatibility

What Stormix 1.0 runs from Postman and Bruno collections, feature by feature. Anything marked ✗ or partial prints a warning on import or when the run starts, so nothing is dropped silently. [Collections](COLLECTIONS.md) explains how collections run.

✓ supported · ◐ partial · ✗ not yet

## Collections

| Feature | Postman | Bruno | Notes |
|---|---|---|---|
| Collection format | ✓ v2.0, v2.1 | ✓ `.bru` folders | Postman v1: export again as v2.1 |
| Folders | ✓ | ✓ | Flattened in run order; steps are named `Folder / Request` |
| Methods | ✓ | ✓ | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS |
| Query parameters | ✓ | ✓ | Disabled ones are left out |
| Path variables | ✓ `:id` | ✓ `params:path` | |
| Headers | ✓ | ✓ | Collection and folder headers in Bruno; disabled ones left out |
| Collection variables | ✓ | ✓ `vars:pre-request` | |
| Environments | ✓ export with `--env` | ✓ `environments/*.bru`, by name | Picked from a list in the app |
| Globals | ◐ | n/a | `pm.globals` works inside a run; global exports are not read |

## Bodies

| Body | Postman | Bruno | Notes |
|---|---|---|---|
| JSON | ✓ | ✓ | |
| Text, XML, other raw | ✓ | ✓ | `Content-Type` follows the body's language |
| URL-encoded form | ✓ | ✓ | |
| Multipart text fields | ✓ | ✓ | |
| Multipart file fields | ✗ | ✗ | Left out, with a warning |
| Binary body | ✗ | ✗ | Left out, with a warning |
| GraphQL | ✓ | ✓ | Sent as JSON `{ query, variables }` |

## Auth

| Auth | Postman | Bruno | Notes |
|---|---|---|---|
| Bearer token | ✓ | ✓ | |
| Basic | ✓ | ✓ | |
| API key, header or query | ✓ | ✓ | |
| Inherited from folder or collection | ✓ | ✓ `inherit` | |
| OAuth 2 | ✗ | ✗ | Sent without credentials, with a warning. Fetch a token in a first step instead |
| Digest, AWS Signature, NTLM, Hawk | ✗ | ✗ | Sent without credentials, with a warning |

## Scripts and tests

| Feature | Postman | Bruno | Notes |
|---|---|---|---|
| Pre-request scripts | ✓ collection, folder, request | ✓ `script:pre-request` at every level | Run in that order |
| Test scripts | ✓ | ✓ `script:post-response`, `tests` | |
| Top-level `await` | ✓ | ✓ | Scripts run as async functions, as in Postman and Bruno |
| `pm.test` / `test` | ✓ | ✓ | Each one is a check in every report |
| Legacy `tests["…"]` | ✓ | n/a | |
| Declarative assertions | n/a | ✓ `assert`, every operator | |
| Post-response variables | via scripts | ✓ `vars:post-response` | |
| Variables API | ✓ `pm.environment`, `pm.variables`, `pm.collectionVariables`, `pm.globals` | ✓ `bru.getVar`, `setVar`, `getEnvVar`, `setEnvVar`, `interpolate` | One store backs every scope |
| Request API | ✓ `pm.request` headers, URL, body | ✓ `req.getUrl`, `setUrl`, `setHeader`, `setBody`, `setMethod` | |
| Response API | ✓ `pm.response` `code`, `json()`, `text()`, `headers`, `to.have.*`, `to.be.ok` | ✓ `res.status`, `res.body`, `getStatus()`, `getBody()`, `getHeader()`, `getResponseTime()` | |
| Chai `expect` | ✓ | ✓ | Common chains: `equal`, `eql`, `include`, `property`, `lengthOf`, `match`, `keys`, `oneOf`, `a`, `above`, `below`, `not`, `deep` |
| `console.log` | ✓ | ✓ | Shown for failed steps, or all steps with `-v` |
| `pm.sendRequest` | ✗ | n/a | The script fails with a clear error. Make it a step of its own |
| `require` (lodash, moment, …) | ✗ | ✗ | The script fails with a clear error |
| `pm.iterationData`, data files | ✗ | ✗ | Always empty until data-driven runs arrive |
| `pm.visualizer` | ✗ | n/a | A call to it fails the script |

Scripts run in an embedded QuickJS engine with no network or file access, a memory cap and a time limit (`--script-timeout`, 5 s by default).

## Request types

| Type | Postman | Bruno |
|---|---|---|
| HTTP/1.1 and HTTP/2 | ✓ | ✓ |
| GraphQL over HTTP | ✓ | ✓ |
| gRPC | n/a, not in v2.1 exports | ✗ skipped, with a warning |
| WebSocket | n/a, not in v2.1 exports | ✗ skipped, with a warning |

## Coming from k6

| k6 | Stormix |
|---|---|
| `export const options = { vus, duration }` | `load: { virtual_users: { count, duration } }` |
| `constant-arrival-rate` executor | `load: { constant_rate: { rate, duration } }` |
| `ramping-arrival-rate` executor | `load: { ramping: { start_rate, stages } }` |
| `thresholds: { http_req_duration: ['p(95)<300'] }` | `thresholds: ["p95 < 300ms"]` |
| `http_req_failed` rate threshold | `errors < 1%` |
| `check(res, { ... })` | `assert:` on the step, or `pm.test` in a script |
| `sleep(1)` between iterations | `think_time: 1s` |
| `k6 run script.js` | `stormix load file.stormix.yaml` |

Stormix flows are declarative YAML rather than JavaScript programs. Loops, branching and custom metrics are not part of 1.0.
