# Collections

`stormix run` takes a Postman collection or a Bruno collection as it is, scripts included. `stormix import` converts one into a Stormix test file when you would rather keep YAML in the repository. The desktop app imports both from the **Import** buttons on the Test file screen.

```sh
stormix run my-api.postman_collection.json --env staging.postman_environment.json
stormix run ./my-bruno-collection --env local          # a Bruno environment, by name
stormix import ./my-bruno-collection -o my-api.stormix.yaml
```

Folders are flattened in run order, and each step is named after its folder path, such as `Users / Create user`. Anything that does not carry over is printed as a warning; nothing is dropped silently.

## What carries over

| | Postman (v2.0, v2.1) | Bruno |
|---|---|---|
| Requests | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS | the same |
| Variables | collection variables; environments with `--env` | `vars:pre-request` of the collection; `environments/*.bru` with `--env NAME` |
| Path variables | `:id` from the URL's variables | `params:path` |
| Headers | request headers; disabled ones are left out | collection, folder and request headers; `~` disabled ones left out |
| Bodies | raw (with the language's `Content-Type`), URL-encoded, form-data text fields, GraphQL | json, text, xml, form-urlencoded, multipart text fields, GraphQL |
| Auth | bearer, basic, API key (header or query); inherited from folders and the collection | bearer, basic, API key; `inherit` from folder and collection |
| Scripts | collection, folder and request pre-request and test scripts, in that order | collection, folder and request `script:pre-request`, `script:post-response` and `tests` |
| Assertions | `pm.test` and legacy `tests["..."]` | `assert` blocks, with every Bruno operator; `vars:post-response` |

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
