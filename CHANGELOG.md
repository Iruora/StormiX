# Changelog

## 1.0.0

The first stable release: one engine for API regression runs and load tests, in a desktop app for macOS, Windows and Linux and in the `stormix` CLI.

### Test files and runs

- YAML test files with a flow of chained requests, variables, extraction (JSONPath, header, regex) and assertions (status, duration, header, JSON, body). See [Test files](docs/TEST_FILE.md).
- `stormix run` sends each step once and reports every check, as text, JSON or JUnit XML.
- HTTP/1.1 and HTTP/2 over rustls, generated values such as `{{$guid}}` and `{{$timestamp}}`, and per-request timeouts.

### Collections

- Postman v2.0 and v2.1 collections and Bruno collection folders run as they are, environments included.
- Pre-request and test scripts run in an embedded, sandboxed QuickJS engine with the `pm.*` and `bru.*` APIs, a Chai-style `expect` and Bruno's declarative `assert` blocks.
- `stormix import` turns a collection into a test file. What does not carry over is reported, never dropped silently. See [Compatibility](docs/COMPATIBILITY.md).

### Load tests

- Constant-rate and ramping open models, and a virtual-user closed model.
- HDR latency histograms (p50 to p99, min, avg, max), per-step figures, the most common failures and a per-second timeline.
- Thresholds on latency, error rate, check rate and throughput set the exit code, so a CI job fails when they do.
- On the same machine, Stormix sent 42.7k requests a second against k6's 22.3k at 50 virtual users, using 13 MB of memory against 153 MB (`bench/compare.sh`, run in CI).

### Desktop app

- Import Postman and Bruno collections, pick an environment, and open or save test files.
- Edit flows as steps (method, URL, headers, body, expected status, load profile, thresholds) or as YAML, with validation as you type.
- **Run once** for a functional run, **Load test** with a live chart of requests, failures and p95.
- Run history with regression diffs: which steps broke or were fixed, which checks changed, and how latency and throughput moved between two load tests.
- Export any run as an HTML report, JSON or JUnit XML.

### Reports

- Self-contained HTML reports, with no scripts or external files, from `--html` on `run` and `load` and from the app.

### Known limits

- Installers are not signed yet.
- `pm.sendRequest`, `require`, OAuth 2 and digest auth, file uploads, gRPC and WebSocket are not supported yet.
- Editing a flow in the Steps tab rewrites its YAML, so comments are dropped.
