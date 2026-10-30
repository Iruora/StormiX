# Stormix

Stress test APIs and run collection flows, from a desktop app or the command line.

Stormix is a free, open source alternative to k6 with a GUI. It does two jobs with one engine:

- **Load and stress testing** of HTTP APIs, with constant-rate, ramping and virtual-user load models, latency percentiles and pass/fail thresholds.
- **Non-regression runs** of the flows in your Postman and Bruno collections: chained requests, variables passed between steps, assertions and the collections' own scripts.

A flow that passes as a regression test can be replayed by hundreds of users as a load test, and the `stormix` CLI runs the same files in CI.

> **Status:** early development (M1). Flows run end to end from the CLI and the desktop app: variables, extraction and assertions, with text, JSON and JUnit reports. Collection import lands in M2 and load testing in M3. See the [roadmap](docs/ROADMAP.md).

## Test files

A test file is YAML: a flow, an optional load profile and thresholds. `stormix init` writes [this example](examples/login.stormix.yaml).

```yaml
version: 1
flow:
  name: Log in, then read the profile
  variables:
    base_url: https://api.example.com
  steps:
    - name: Log in
      request:
        method: POST
        url: "{{base_url}}/login"
        body:
          json: { user: demo, password: secret }
      extract:
        token: { json: $.token }
      assert:
        - status: 200
    - name: Read the profile
      request:
        method: GET
        url: "{{base_url}}/me"
        headers:
          - { name: Authorization, value: "Bearer {{token}}" }
load:
  ramping:
    stages:
      - { duration: 30s, target: 20 }
      - { duration: 1m, target: 20 }
thresholds:
  - p95 < 500ms
  - errors < 1%
```

| Section | Options |
|---|---|
| `extract` | `json` (JSONPath), `header`, `regex` |
| `assert` | `status`, `max_duration_ms`, `header` (`equals`, `contains`), `json` (`path`, `equals`, `exists`), `body_contains` |
| `load` | `constant_rate` (`rate`, `duration`), `ramping` (`start_rate`, `stages`), `virtual_users` (`count`, `duration`, `think_time`) |
| `thresholds` | `p50`…`p99`, `avg`, `max` (ms or s), `errors`, `checks` (%), `rps` |

`{{name}}` is filled in from, last one winning: the flow's `variables`, an `--env` file, `--var` on the command line, then values earlier steps extract. `{{$guid}}`, `{{$timestamp}}`, `{{$timestampMs}}` and `{{$randomInt}}` are generated for each use.

## CLI

```sh
stormix init                                   # write stormix.yaml
stormix validate stormix.yaml
stormix run stormix.yaml --env staging.json --var password=secret --junit report.xml
```

`run` sends each step once and prints what failed:

```
Log in, then read the profile

  ✓ Log in            POST    200    134 ms
  ✗ Read the profile  GET     401     12 ms
      ✗ status is 200: got 401

2 steps: 1 passed, 1 failed · 3/4 checks passed · 0.15 s
```

| Option | |
|---|---|
| `--var NAME=VALUE` | Set a variable; repeatable |
| `--env FILE` | Variables from a YAML or JSON map, or a Postman environment export |
| `--junit FILE` | Also write a JUnit XML report for your CI's test tab |
| `--format json` | Print the full result as JSON |
| `-v`, `--verbose` | Show passed checks and extracted values too |
| `--bail` | Stop at the first failed step |
| `--timeout 30s`, `--insecure`, `--no-follow-redirects` | Per-request timeout, accept invalid TLS certificates, report redirects instead of following them |

Both commands exit with 0 on success, 1 when a check or validation fails (warnings too with `validate --strict`), and 2 when the file cannot be read or run. From a checkout, use `cargo run -p stormix-cli -- <command>`.

## Development

You need Rust (stable), Node 22 and pnpm. On Linux, also the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/#linux).

```sh
cd apps/desktop
pnpm install
pnpm tauri dev                 # run the desktop app
cd ../..
cargo test --workspace         # also regenerates apps/desktop/src/bindings.ts
cargo clippy --workspace --all-targets -- -D warnings
```

## Repository layout

```
apps/desktop/        Tauri app: src-tauri/ (Rust shell, IPC) and src/ (React UI)
crates/flow/         flow model, test file format, validation (no Tauri deps)
crates/engine/       runs flows: HTTP client, variables, extraction, assertions, JUnit
crates/cli/          `stormix`, the command line for CI and quick runs
examples/            example test files
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
