# Changelog

## 1.6.0 (2026-11-01)

### Flows

- **GraphQL bodies.** A step can send `body: { graphql: { query, variables, operation_name } }`, sent as JSON with `Content-Type: application/json`. A templated variable takes the type the query declares, so `id: "{{user_id}}"` goes out as the number `7` for `$id: Int!` instead of the text `"7"` that GraphQL servers reject. Variables can also be JSON text with bare `{{variables}}` in it, filled in before it is parsed. See [GraphQL](docs/TEST_FILE.md#graphql).
- A GraphQL step fails when the response has GraphQL `errors`, even with status 200, as well as on a 4xx or 5xx or a body that is not JSON, through a **no GraphQL errors** check that shows the first error's message. Load tests count these as errors. `allow_errors: true` turns the check off for a step that expects errors.
- `stormix validate` and the editor report an empty GraphQL query and variables that are not a JSON object, and warn about undefined variables used in the query and variables.

### Collections

- GraphQL requests from Postman and Bruno are imported as GraphQL bodies instead of raw JSON. Variables written with bare `{{variables}}`, like `{ "id": {{userId}} }`, were not valid JSON and were silently sent as `null`; they are now kept and filled in when sent, and variables that would still not be JSON are reported when importing.

### Desktop app

- The Steps tab has a **graphql** body with fields for the query and variables, the operation name and **pass even with GraphQL errors**. Switching a JSON or raw body holding `{ "query": ... }` to **graphql** converts it, which also upgrades GraphQL requests imported by earlier versions. The YAML editor completes the `graphql` keys.

## 1.5.1 (2026-11-01)

### Desktop app

- **Feedback while things happen.** A strip under the toolbar says what the app is doing, with a progress bar: importing a collection (a Bruno folder counts its files as it reads them, "37 of 120 files", with the file being read), opening, reopening and saving a file, importing an environment, and running. **Run once** shows which step it is waiting for, step by step, in the results panel and the strip. A load test says it is starting (setup steps and warm-up) until its first second is in. The button that started the action shows a spinner, and buttons that would clash with it are disabled until it finishes. Saving shows **Saved** beside the file name. In Runs, exporting a report or deleting a run shows a spinner, and loading a run or a comparison says so. Downloading an update shows a progress bar.
- Importing a collection, opening and saving files, reading environments and exporting reports now run off the window's main thread, so the window no longer freezes while a large collection loads.

## 1.5.0 (2026-11-01)

### Desktop app

- **Workspaces.** The app keeps what you have open in a workspace, saved as you work, and opens the last one used when it starts instead of the example. A workspace holds the test file, or the editor text when it is not saved to a file (an imported collection, say), with changes not yet saved to the file; every environment in the list and the one chosen; the editor tab and page; and the steps left out of the last load test. On reopening, the file is read again from disk, along with the environments beside it; if it is gone, the copy kept with the workspace opens instead. The new **Workspaces** page lists them with their file, environment and when they were last saved, to open, rename or delete; **New workspace** starts one from the example. Workspaces are kept in `workspaces.json` in the app's data folder. Secret values are never saved, and neither are changes made for this session only in **Variables**.
- **Variables** next to the environment picker opens the chosen environment's variables to see and change. A change applies to runs at once, for this session only, and is marked **this session** (the toolbar button shows how many). Save one variable, or **Save all to file**, to make the change permanent; reset one, or **Reset all**, to go back to the file. Variables can also be added and removed, for the session or for good. Saving writes the environment's own file in its own format (`.bru`, Postman export, JSON or YAML), keeping a `.bru` file's disabled lines and a Postman export's other fields; an environment that came with a collection is saved to `environments/NAME.yaml` next to the test file, or asks where. Secrets stay masked, and their values are never written to a file.

## 1.4.0 (2026-11-01)

### Desktop app

- The Flow tab has an **All steps** switch that turns every step on, or, when they are all on, turns them all off. It replaces the **Turn all on** button.
- The layout follows the system text size and display scaling. Text, icons, spacing and the results panel are sized in rem, so a larger text setting scales the whole app evenly instead of only part of it. The header and the editor toolbar wrap onto more lines instead of pushing **Run once** and **Load test** off screen. The results panel stays at least 18 rem wide and always leaves the editor at least 26 rem; when the window is too small for both they share it. The Flow tab puts the Set aside tray under the flow when it is narrow, and step rows, run results and load test figures wrap instead of overlapping.

### Flows

- Steps can cache their response with `cache: true`. A cached step is sent once; later runs and load test iterations reuse the response (checks, extraction and test scripts still run on it) until the cache is cleared. Only passing responses are kept, one per distinct request. In a load test, cached steps are prerequisites (like a login): they run once before the load starts and stay out of the metrics and thresholds; reused responses are counted apart from requests. Files without `cache` run as before.
- Desktop app: turn caching on or off per step in the Flow tab (database icon), the Steps tab and the load test setup dialog. Steps with a cached response show **cached**; clear one step's cache there, or the whole flow's with **Clear cache**. Results mark steps that came from the cache.

## 1.3.1 (2026-11-01)

### Downloads

- Each release has a `Stormix_<version>_x64_installer.zip` for Windows: the setup `.exe` and the `.msi` in one zip, with a short README, for places that only take or let through `.zip` downloads.

## 1.3.0 (2026-11-01)

### Desktop app

- **Load test** opens a setup dialog first. It lists the flow's endpoints, all selected, with a checkbox each and one to select or clear them all; it warns when a selected step uses a variable only a left-out step extracts. The load profile and thresholds are edited there too and saved into the test file. **Run** closes the dialog and starts the test with the selected steps; **Cancel** closes it. A file without a `load` section can now be load tested: the dialog starts from 10 iterations/s for 30 s.
- The per-step table of a load test result sorts by any column (click a header; again to flip, a third time for flow order) and filters by step name, failing steps only, and a minimum p95.

### Flows

- Steps can be turned off with `enabled: false`. A turned off step keeps its place but is not sent by runs or load tests, and does not appear in results. Steps can also be moved under `flow.set_aside`, out of the flow altogether, without deleting them.
- Desktop app: a **Flow** tab shows the steps as a diagram. Turn steps on and off, drag them to reorder or to a **Set aside** tray (and back), duplicate, delete (with undo) or add a step between two others. The load test setup dialog leaves out steps that are turned off. Steps show their last run result, and a step warns when a variable it uses comes from a step that is turned off, set aside or now runs after it.

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
