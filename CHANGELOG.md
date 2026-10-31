# Changelog

## 1.1.0 (2026-10-31)

### Desktop app

- **Auto-update.** At launch the app checks the latest GitHub release and, when there is a newer version, offers to install it and restart. Update bundles are signed, and the app only installs ones signed with the project's key. Updates start working from 1.1.0; people on 1.0 download 1.1 by hand once.
- The YAML tab is a code editor (CodeMirror): highlighting, line numbers, folding, search, and errors underlined in place as you type. Syntax errors are found on each keystroke; format errors (an unknown key, a wrong method, a bad value) and warnings from the validator are placed on the key or value they are about. The status bar counts problems and jumps to the next one (F8). Ctrl/Cmd+S saves.
- Completion of the test file's keys where the cursor is, of methods, thresholds, and `{{variables}}` (the flow's, extracted ones, the chosen environment's and generated ones).
- `{{variables}}` are coloured in the steps editor and the YAML editor (defined, set at run time by an extraction or a script, generated, or undefined), and hovering one shows its value and where it comes from, including what the last run extracted.
- Each HTTP method has its own colour in the method selector and wherever a method is shown.
- Fixed: switching a body between none, json, raw and form in the steps editor dropped its content, for example a Bruno JSON body imported as raw. JSON text now carries over between json and raw, each kind remembers its content, and templated JSON that does not parse yet stays raw with a note.
- A **Details** button on each step of a run, and on each of a load test's most common failures, opens the failed checks, the error, the request as sent (headers and body) and the response as received, with JSON bodies indented and a **Copy as cURL** button.

### Engine

- Each step result can carry its `exchange`: request headers and body as sent and the response status text, headers and body, bodies cut at 64 KB. The app keeps it for every step; load tests and the CLI keep it only for failed steps, so a `--format json` report shows what a failing request sent and got back.
- Load reports carry `samples`: the first failure of each step, with its exchange. Runs saved by 1.0 still open.

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
