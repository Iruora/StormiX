# Collections

`stormix run` takes a Postman collection or a Bruno collection as it is, scripts included. Bruno collections can be in either of Bruno's formats: `.bru` files, or [OpenCollection](https://www.opencollection.com) YAML, the format Bruno saves in since v3. `stormix import` converts one into a Stormix test file when you would rather keep YAML in the repository, and `stormix export` goes the other way, writing a test file as an OpenCollection that Bruno opens or as a Postman collection. The desktop app imports all of them from the **File** menu (**Import a collection**), and exports the open test file from **File › Export as**: a Bruno folder, an OpenCollection file or a Postman collection.

```sh
stormix run my-api.postman_collection.json --env staging.postman_environment.json
stormix run ./my-bruno-collection --env local          # a Bruno environment, by name
stormix import ./my-bruno-collection -o my-api.stormix.yaml   # also writes environments/local.yaml
stormix run my-api.stormix.yaml --env local --var token=...
stormix run ./my-opencollection --env local             # opencollection.yml in it
stormix run my-api.yml                                  # a bundled OpenCollection file
stormix export my-api.stormix.yaml -o ./for-bruno       # a folder Bruno opens
stormix export my-api.stormix.yaml -o my-api.json        # a Postman collection
```

## OpenCollection

An OpenCollection is a folder with `opencollection.yml` at its root, a `folder.yml` in each sub-folder, one `.yml` file per request and environments in `environments/`, or a single bundled `.yml` file holding all of it (environments under `config.environments`). Stormix reads both: pass the folder, its `opencollection.yml`, or the bundled file. A `.yml` file counts as an OpenCollection only when it has a top-level `opencollection:` key, so Stormix test files are never mistaken for one.

It imports exactly like the same collection saved as `.bru` files: collection and folder headers, auth, variables and scripts are applied to each request under them, `set-variable` actions are Bruno's post-response variables, and assertions take every Bruno operator. Disabled headers, parameters, form fields, variables and assertions are left out. Where a body or variable has several variants, the selected one is used. An environment that `extends` another gets the other's variables under its own.

### Exporting to Bruno

`stormix export FILE -o DIR` writes `FILE` (a test file, or any collection Stormix reads) as an OpenCollection folder, with the environments next to the test file in `environments/`. `-o api.yml` writes one bundled file instead. In the desktop app, **File › Export as › Bruno folder** writes the open test file and its environments to a folder you choose, and **OpenCollection file** writes the bundled file. Then open it in Bruno.

- Steps named `Folder / Request` go into folders.
- `assert` checks become Bruno assertions: `status` is `res.status eq`, `max_duration_ms` is `res.responseTime lte`, `header` checks read `res.headers["name"]`, and `json` checks read the path under `res.body`. `body_contains` becomes a `test` in the tests script.
- `extract` becomes post-response variables (`set-variable` actions) for JSON paths and headers, and a line of post-response script for a regex.
- Scripts go over as they are, as `before-request` and `tests` scripts.
- Environment secrets are written by name only, as Bruno secrets.

What Bruno has no place for is left out and printed as a warning: steps that are turned off or set aside, `cache`, `trace`, `allow_errors` and `operation_name` on GraphQL bodies, the load profile and thresholds, and JSON paths with wildcards, filters or `..`. Scripts written for Postman (`pm.*`) are exported but warned about, since Bruno runs only part of that API. Bruno runs a folder's own requests before its sub-folders; when the steps are ordered otherwise, the export says so.

### Exporting to Postman

`stormix export FILE -o NAME.json` writes a Postman collection (v2.1), and each environment next to it as `ENV.postman_environment.json`. In the desktop app, use **File › Export as › Postman collection**. Import the collection and the environment files in Postman.

- Steps named `Folder / Request` go into folders. Postman runs a folder's requests together, so when a folder's steps are not next to each other, the export says the order changes.
- `assert` checks become `pm.test` lines in the request's Tests script: `pm.response.to.have.status`, `pm.response.responseTime`, `pm.response.headers.get` and `pm.response.json()` for `json` checks.
- `extract` sets collection variables after the response (`pm.collectionVariables.set`), so later requests read them as `{{name}}`.
- Scripts go over as they are, as Pre-request and Tests scripts. Scripts written for Bruno (`bru.*`) are exported but warned about.
- Environment secrets are written by name only, as Postman secret variables with no value.

As with Bruno, what Postman has no place for is left out and printed as a warning: steps that are turned off or set aside, `cache`, `trace`, `allow_errors` and `operation_name` on GraphQL bodies, the load profile and thresholds, and JSON paths with wildcards, filters or `..`.

## Environments

An environment file is a flat YAML or JSON map of names to values, a Postman environment export, or a Bruno environment: a `.bru` file or an OpenCollection `.yml` (`name` and a `variables` list). Saving changes to an OpenCollection environment keeps it in that format. `--env` takes a path, or a name: one of a Bruno collection's `environments/`, or a file in the `environments/` folder next to a test file. That is where `stormix import -o` writes a collection's environments. Secrets (Bruno's `vars:secret`) have no value in the file; in YAML their names are listed under `$secrets`:

```yaml
base_url: https://staging.example.com
$secrets: [token]
```

In the desktop app, the environment menu in the header picks the environment and has **Import environment** to add one from any of these files. Opening a test file also loads the environments next to it, and **Secrets** in the same menu asks for the values of the chosen environment's secrets. Those values stay in memory and are never written to a file. Environments stay in the list when you open another file, until you remove them.

Folders are flattened in run order, and each step is named after its folder path, such as `Users / Create user`. Anything that does not carry over is printed as a warning; nothing is dropped silently.

## What carries over

| | Postman (v2.0, v2.1) | Bruno (`.bru` or OpenCollection) |
|---|---|---|
| Requests | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS | the same |
| Variables | collection variables; environments with `--env` | `vars:pre-request` (OpenCollection: `request.variables`) of the collection; `environments/*.bru` or `*.yml` with `--env NAME` |
| Path variables | `:id` from the URL's variables | `params:path` |
| Headers | request headers; disabled ones are left out | collection, folder and request headers; `~` disabled ones left out |
| Bodies | raw (with the language's `Content-Type`), URL-encoded, form-data text fields, GraphQL | json, text, xml, form-urlencoded, multipart text fields, GraphQL |
| Auth | bearer, basic, API key (header or query); inherited from folders and the collection | bearer, basic, API key; `inherit` from folder and collection |
| Scripts | collection, folder and request pre-request and test scripts, in that order | collection, folder and request `script:pre-request`, `script:post-response` and `tests` |
| Assertions | `pm.test` and legacy `tests["..."]` | `assert` blocks (OpenCollection: `runtime.assertions`), with every Bruno operator; `vars:post-response` (`set-variable` actions) |

Scripts run in an embedded JavaScript engine (QuickJS) as async functions, so top-level `await` works. They have no file access and can only reach the network through `sendRequest`. They have a memory cap and a time limit (`--script-timeout`, 5 s by default). Time spent waiting on `bru.sleep` and `sendRequest` does not count toward it; waiting has its own cap of 5 minutes per script. They see:

- **Postman:** `pm.environment`, `pm.variables`, `pm.collectionVariables`, `pm.globals`, `pm.test`, `pm.expect`, `pm.request` (headers, URL, body), `pm.response` (`code`, `status`, `responseTime`, `text()`, `json()`, `headers`, `to.have.status`, `to.have.header`, `to.have.jsonBody`, `to.be.ok`…), and the legacy `postman.*`, `tests`, `responseBody`, `responseCode`, `responseTime` and `responseHeaders`.
- **Bruno:** `bru.getVar`/`setVar`/`getEnvVar`/`setEnvVar`/`interpolate`…, `req.getUrl`/`setUrl`/`setHeader`/`setBody`/`setMethod`…, `res.status`, `res.body`, `res.headers`, `res.getStatus()`, `res.getBody()`, `res.getHeader()`, `res.getResponseTime()`, `bru.sleep`, `bru.sendRequest` (axios-style), `test` and `expect`.
- **Both:** a Chai-style `expect` (`equal`, `eql`, `above`, `below`, `include`, `property`, `lengthOf`, `match`, `keys`, `oneOf`, `a`/`an`, `ok`, `empty`, `exist`, `not`, `deep`…), `console.log`, `btoa`/`atob`, `Buffer` (utf8, base64, base64url, hex) and `pm.sendRequest`.

Requests sent from a script use the run's client settings. They are not counted in load-test metrics. A retry loop that pauses with `bru.sleep` works under the default limit, since only running time counts.

Every `pm.test`, `test`, `tests[...]` and `assert` line is reported as a check of the same name, in the terminal, the JSON output and the JUnit report. A script that throws fails its step with the error. `console.log` output shows for failed steps, and for all steps with `-v`.

One variable store backs every scope (environment, collection, globals, request), so a value set in any of them is seen by all the steps after it.

## Not yet supported

- `require`: a script that uses it fails with a clear error. `pm.iterationData` is always empty until data-driven runs arrive.
- OAuth 2, digest, AWS and NTLM auth: the request is sent without credentials, with a warning.
- File uploads (form-data file fields, binary bodies): left out, with a warning.
- Postman collection v1 exports: export again as v2.1.
- Bruno gRPC and WebSocket requests: skipped, with a warning.
