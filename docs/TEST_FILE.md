# Test files

A Stormix test file is YAML with three parts: a **flow** (the requests), an optional **load** profile and optional **thresholds**. Files usually end in `.stormix.yaml`. `stormix validate` checks one without sending anything, and `stormix init` writes an example to start from.

```yaml
version: 1
flow:
  name: Log in, then read the profile
  description: Smoke test for the auth service   # optional
  variables:
    base_url: https://api.example.com
    user: demo
  steps:
    - name: Log in
      request:
        method: POST
        url: "{{base_url}}/login"
        body:
          json: { user: "{{user}}", password: "{{password}}" }
      extract:
        token: { json: $.token }
      assert:
        - status: 200
        - header: { name: content-type, contains: json }
    - name: Read the profile
      request:
        method: GET
        url: "{{base_url}}/me"
        headers:
          - { name: Authorization, value: "Bearer {{token}}" }
      assert:
        - status: 200
        - json: { path: $.user, equals: "{{user}}" }
        - max_duration_ms: 500
load:
  ramping:
    start_rate: 0
    stages:
      - { duration: 30s, target: 50 }
      - { duration: 2m, target: 50 }
      - { duration: 30s, target: 0 }
thresholds:
  - p95 < 300ms
  - errors < 1%
  - checks > 99%
```

## `version`

Always `1` for now. A file with an unknown version is rejected rather than misread.

## `flow`

| Field | Required | |
|---|---|---|
| `name` | yes | Shown in reports and in the run history |
| `description` | no | Free text |
| `variables` | no | A map of names to string values, used as `{{name}}` |
| `steps` | yes | One or more steps, run in order |
| `set_aside` | no | Steps kept with the file but out of the flow. They are never run or checked; the desktop app's Flow view puts them back |
| `trace` | no | `request` or `flow`: send a W3C `traceparent` header with every request, for Dynatrace and other APM tools. See [Tracing requests](#tracing-requests) |

One virtual user runs the steps in order. In a load test, each iteration is one pass through all the enabled steps, with its own copy of the variables.

### Turning steps off

A step with `enabled: false` keeps its place but is not sent: runs and load tests go straight from the step before it to the step after it, and it does not appear in results. Variables it would extract are not set, so later steps that use them warn. To take a step out of the order altogether without deleting it, move it under `set_aside`:

```yaml
flow:
  name: checkout
  steps:
    - name: Log in
      request: { method: POST, url: "{{baseUrl}}/login" }
    - name: Apply coupon
      enabled: false
      request: { method: POST, url: "{{baseUrl}}/coupon" }
    - name: Pay
      request: { method: POST, url: "{{baseUrl}}/pay" }
  set_aside:
    - name: Old payment endpoint
      request: { method: POST, url: "{{baseUrl}}/v1/pay" }
```

### Tracing requests

APM tools like Dynatrace, Datadog, New Relic, Elastic, Jaeger and Grafana Tempo follow a request through every service it reaches by a trace ID that travels in a header. With `trace` set, Stormix starts the trace: every request carries a [W3C `traceparent`](https://www.w3.org/TR/trace-context/#traceparent-header) header with a new span ID, marked as sampled, and every result records the trace ID it was sent with. A slow or failed request in Stormix can then be looked up in the APM tool to see where its time went.

```yaml
flow:
  name: checkout
  trace: request
  steps: ...
```

- `trace: request` gives every request a trace of its own, so one trace ID means one request.
- `trace: flow` makes each run of the flow (each load test iteration) one trace, with a span per request, so a whole journey (log in, add to cart, pay) shows as one trace.
- A step that sets its own `traceparent` header keeps it, and its trace ID is recorded too. One that is not valid W3C trace context is sent as written, with a warning.
- A response reused from the cache was not sent, so it has no trace ID. The header does not stop a cached step from matching.
- Run results carry `traceId` and `spanId` per step, in JSON, Details, the HTML report and JUnit failures. Load tests keep the 5 slowest traced requests of each step (`slowest` in the JSON report, also in the HTML report and the desktop app), and failure samples keep theirs.
- `stormix run` and `stormix load` take `--trace [request|flow|off]` to override the file.

In Dynatrace, OneAgent reads `traceparent` when W3C Trace Context is turned on (it is by default in recent versions), and the trace ID can be searched for under **Distributed traces**. The services behind the API must be monitored for their part to show; the request itself starts at Stormix, which reports nothing to the APM tool.

### Caching a step

A step with `cache: true` is sent once; after that, runs and load test iterations reuse its response instead of sending the request again. This suits a login or a lookup that every iteration needs but that should not be part of the load.

```yaml
    - name: Log in
      cache: true
      request: { method: POST, url: "{{baseUrl}}/login", body: { json: { user: demo, password: "{{password}}" } } }
      extract: { token: { json: $.token } }
```

- A response is kept per distinct request: the same method, URL, headers and body once variables are filled in. Change a variable the request uses and it is sent (and kept) again.
- Only a response that passed is kept: no error, every check passing, and not a 4xx or 5xx that nothing checks. A failed login is retried on the next run.
- Pre-request scripts still run first. Assertions, extraction and test scripts run on the reused response, so variables like a token are still set.
- In a load test, cached steps are prerequisites: before the load starts, Stormix runs the flow once up to the last cached step (so a cached step still gets the variables of the steps before it), and every iteration then reuses those responses. That setup pass is reported on its own (`setup` in the JSON report) and is not in the summary or the thresholds. Reused responses are counted apart (`cached_responses`, and per step), not as requests, and stay out of the latency figures.
- Reused responses are marked `cached` in results.
- The cache is kept in memory, for as long as the desktop app (or one `stormix` command) runs. In the desktop app, clear it for one step or for the whole flow from the Flow tab, the Steps tab or the load test setup dialog.
- Steps without `cache` behave as before: Stormix does not cache HTTP responses otherwise, and sends no cache headers of its own. To ask servers and proxies not to answer from their caches, add a `Cache-Control: no-cache` header to the request.

## Steps

| Field | Required | |
|---|---|---|
| `name` | yes | Unique names make reports and comparisons clearer |
| `enabled` | no | `false` skips the step in runs and load tests; defaults to `true` |
| `cache` | no | `true` reuses the step's response instead of sending the request again; defaults to `false`. See [Caching a step](#caching-a-step) |
| `request` | yes | See below |
| `extract` | no | Variables to set from the response |
| `assert` | no | Checks on the response |
| `scripts` | no | `pre_request` and `test` JavaScript, in the Postman or Bruno style |

### `request`

| Field | |
|---|---|
| `method` | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD` or `OPTIONS` |
| `url` | The full URL; query strings go here too |
| `headers` | A list of `{ name, value }`. A list rather than a map, because HTTP allows a header to repeat |
| `body` | One of the five kinds below |

| Body | Sent as |
|---|---|
| `json: <any YAML value>` | `application/json`, unless a `Content-Type` header says otherwise |
| `raw: <text>` | As written, with the step's own `Content-Type` header |
| `form: [{ key, value }]` | `application/x-www-form-urlencoded` |
| `multipart: [{ key, value }]` | `multipart/form-data`, text fields only |
| `graphql: { query, variables }` | `application/json` `{ query, variables, operationName }`. See [GraphQL](#graphql) |

#### GraphQL

```yaml
- name: Get a user
  request:
    method: POST
    url: "{{base_url}}/graphql"
    body:
      graphql:
        query: |
          query User($id: Int!) {
            user(id: $id) { id name }
          }
        variables: { id: "{{user_id}}" }
  extract:
    name: { json: $.data.user.name }
```

| Field | |
|---|---|
| `query` | The GraphQL document |
| `variables` | An object. Literal values keep their YAML type. A templated value such as `"{{user_id}}"` is sent as the type the query declares for that variable: a number for `Int` and `Float`, `true` or `false` for `Boolean`, an object or list for an input type or a list (from JSON text), and text for `String`, `ID`, enums and anything else. Only top-level variables have a declared type; `{{name}}` inside a nested object is sent as text |
| | Can also be JSON text, as Postman and Bruno keep it: `variables: '{ "id": {{user_id}} }'`. Variables are filled in first, then the text is parsed, so a bare `{{name}}` is sent as whatever its value is. Text that is not JSON once filled in fails the step before it is sent |
| `operation_name` | Which operation to run when the query holds several |
| `allow_errors` | `true` passes the step even when the response has GraphQL `errors`. Defaults to `false` |

GraphQL servers usually answer `200 OK` even when the operation failed, listing what went wrong in an `errors` array. So a GraphQL step gets a check of its own, **no GraphQL errors**, that fails when the response has a non-empty `errors` array, has a 4xx or 5xx status, or is not JSON. Its message is the first error's message and path. Like any failed check, it counts toward `errors` and `checks` in load tests. Set `allow_errors: true` for a step that expects errors, and check them with `json` assertions, like `json: { path: "$.errors[0].message", equals: Not found }`.

### `extract`

Each entry sets a variable for the steps after it. A source that matches nothing fails the step, so a missing token shows up where it went missing.

| Source | Example | Takes |
|---|---|---|
| `json` | `token: { json: $.data.token }` | A JSONPath into the body. Strings come out as they are; other values as JSON |
| `header` | `etag: { header: ETag }` | A response header, matched case-insensitively |
| `regex` | `csrf: { regex: 'name="csrf" value="([^"]+)"' }` | The first capture group, else the whole match |

### `assert`

| Assertion | Passes when |
|---|---|
| `status: 200` | The status code is exactly 200 |
| `max_duration_ms: 500` | The full response arrived within 500 ms |
| `header: { name: content-type }` | The header is present |
| `header: { name: content-type, equals: application/json }` | It equals the value exactly |
| `header: { name: content-type, contains: json }` | It contains the text |
| `json: { path: $.items }` | The JSONPath matches something |
| `json: { path: $.items[0].id, equals: 7 }` | The match equals the value (any YAML value; `"7"` also matches the number 7) |
| `json: { path: $.error, exists: false }` | The path matches nothing |
| `body_contains: "ok"` | The body contains the text |

Variables work in every value, such as `equals: "{{user}}"`. With no assertion on a step, a 4xx or 5xx status still counts as a failed request under load, but a functional run passes it.

### `scripts`

```yaml
scripts:
  pre_request: |
    pm.variables.set("ts", Date.now().toString());
  test: |
    pm.test("has an id", () => pm.expect(pm.response.json().id).to.be.a("number"));
```

Each `pm.test` or `test` becomes a check with that name. Scripts run sandboxed, with no file access, and reach the network only through `sendRequest`. [Collections](COLLECTIONS.md) lists the API they can use.

## Variables

`{{name}}` is replaced when the request is sent, in the URL, headers, body, assertion values and scripts' `pm.variables`. Values come from these places, and a later one wins:

1. The flow's `variables`
2. An environment: `--env FILE` on the CLI, or the environment picked in the app
3. `--var NAME=VALUE` on the CLI
4. Values that earlier steps extract or scripts set

These are generated fresh each time they are used:

| Variable | Value |
|---|---|
| `{{$guid}}`, `{{$randomUUID}}` | A random UUID v4 |
| `{{$timestamp}}` | Unix time in seconds |
| `{{$timestampMs}}` | Unix time in milliseconds |
| `{{$randomInt}}` | An integer from 0 to 1000 |

A `{{name}}` that has no value is sent as written. The run shows a warning on that step, and `validate` warns about it beforehand.

An environment file is a YAML or JSON map (`base_url: https://staging.example.com`), a Postman environment export, or a Bruno `.bru` environment.

## Load profiles

Without a `load` section, the flow runs once as a functional test. With one, `stormix load` and the app's **Load test** button run it under that profile. `stormix run` still runs it once.

| Model | Fields | Use it to |
|---|---|---|
| `constant_rate` | `rate`, `duration` | Hold a steady arrival rate: `rate` iterations start each second, however slow the API gets |
| `ramping` | `start_rate` (default 0), `stages: [{ duration, target }]` | Ramp up, hold and ramp down. The rate moves linearly to each stage's `target` over its `duration` |
| `virtual_users` | `count`, `duration`, `think_time` (optional) | Simulate a fixed number of users who each loop the flow, pausing `think_time` between loops |

`constant_rate` and `ramping` are *open* models: the load does not drop when the API slows down, which is what real traffic does, and what exposes queueing. `virtual_users` is a *closed* model: a slow API gets fewer requests. In open models, if 1000 iterations (`--max-in-flight`) are already running, new ones are dropped and counted rather than delayed.

Durations are whole numbers with a unit: `250ms`, `30s`, `2m`, `1h`.

```yaml
load:
  constant_rate: { rate: 100, duration: 5m }
```

```yaml
load:
  virtual_users: { count: 25, duration: 10m, think_time: 1s }
```

## Thresholds

A threshold is `<metric> <operator> <value>`. The operator is `<`, `<=`, `>` or `>=`. When a threshold fails, `stormix load` exits with 1, so a CI job fails too.

| Metric | Value | Example |
|---|---|---|
| `p50`, `p90`, `p95`, `p99`, or any `pNN` | Latency, in `ms` or `s` | `p95 < 300ms`, `p99 < 1s` |
| `avg`, `max` | Latency, in `ms` or `s` | `avg < 100ms` |
| `errors` | Failed requests, in percent | `errors < 1%` |
| `checks` | Passed assertions and script tests, in percent | `checks > 99%` |
| `rps` | Requests per second over the run, a plain number | `rps > 200` |

Thresholds apply only under load. `stormix load --threshold "p95 < 300ms"` adds one from the command line.
