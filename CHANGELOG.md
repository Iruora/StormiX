# Changelog

## 1.2.0 (2026-11-01)

### Desktop app

- The sidebar is a narrow column of icons; each one's label shows on hover.
- The results panel on the right of the Test file screen can be hidden with the toggle at the end of the editor toolbar, and resized by dragging its edge (arrow keys work too; double-click resets it). Its width and state are remembered. It opens by itself when a run starts, and while hidden the toggle shows a dot when there are errors or failures.

### Environments

- Desktop app: **Import env** adds an environment from a file of its own: a YAML or JSON map, a Postman environment export or a Bruno `.bru` environment. Before, environments came only from a Bruno collection.
- Desktop app: a **Secrets** button asks for the values of an environment's secrets (Bruno's `vars:secret`), the app's `--var`. The values stay in memory only.
- Desktop app: environments are no longer cleared when you open a file or start from the example. Opening a test file also loads the environments in the `environments/` folder next to it. Any environment can be removed from the list.
- `stormix import -o FILE` writes the collection's environments to `environments/NAME.yaml` next to the test file, and `--env NAME` finds them there. Secret names are kept under `$secrets`.

### Scripts

- The script time limit (`--script-timeout`, 5 s by default) counts running time only. Waiting on `bru.sleep` and on requests sent with `sendRequest` no longer counts, so retry loops work in the desktop app too. Those waits are capped at 5 minutes per script.

## 1.1.1 (2026-11-01)

### Scripts

- Scripts run as async functions, as in Bruno and Postman, so `await` at the top level works. Collections that used it failed before with "SyntaxError: expecting ';'". A rejected `await` becomes the script's error, and a script that waits on something that never settles says so.
- `bru.sendRequest` and `pm.sendRequest` send requests from a script, with the run's client settings and within the script's time limit. `bru.sendRequest` resolves like axios (`status`, `statusText`, `headers`, `data`); `pm.sendRequest` takes a URL or a Postman request, with a callback or as a promise.
- `bru.sleep(ms)` waits, never past the script's time limit.
- A `Buffer` for encoding helpers: `Buffer.from(value, enc).toString(enc)` with utf8, base64, base64url, hex, latin1/binary and ascii, plus `isBuffer`, `byteLength` and `concat`.
- `req.getBody({ raw: true })` returns the body's raw text.

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
