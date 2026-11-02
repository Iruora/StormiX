# CLI reference

`stormix` runs the same files as the desktop app, for CI and quick runs from a terminal. Every command takes `--help`.

```sh
stormix init [FILE]                 # write an example test file (stormix.yaml)
stormix validate FILE               # check a file without sending anything
stormix run FILE                    # run the flow once, as a functional test
stormix load FILE                   # run it under load and check thresholds
stormix diagnose FILE --step NAME   # find where one step's request fails
stormix import COLLECTION -o FILE   # turn a Postman or Bruno collection into a test file
stormix export FILE -o DIR          # write a test file as an OpenCollection for Bruno
```

`FILE` is a test file, a Postman collection export (`.json`), a Bruno collection folder (`.bru` files or OpenCollection YAML) or a bundled OpenCollection `.yml`, for `validate`, `run`, `load` and `diagnose` alike.

## Exit codes

| Code | `validate` | `run` | `load` |
|---|---|---|---|
| 0 | No errors | Every check passed | Every threshold held |
| 1 | Errors (and warnings with `--strict`) | A step or check failed | A threshold failed |
| 2 | The file could not be read | The run could not start: unreadable file, errors in it, bad options | The same |

## Options for `run`, `load` and `diagnose`

| Option | |
|---|---|
| `--var NAME=VALUE` | Set a variable, overriding the file and the environment. Repeatable |
| `--env FILE\|NAME` | Variables from a YAML or JSON map, a Postman environment export or a Bruno `.bru` environment. A name works too: one of a Bruno collection's environments, or a file in the `environments/` folder next to a test file |
| `--timeout 30s` | Time allowed for each request |
| `--insecure` | Accept invalid or self-signed TLS certificates |
| `--proxy system\|none\|URL` | Which proxy requests go through. `system` (the default) follows the environment variables, then the Windows or macOS settings; `none` sends every request straight to the server; a URL like `http://proxy.example:8080` sends every request through that proxy. See [Proxies](#proxies) |
| `--no-follow-redirects` | Report redirects instead of following them |
| `--script-timeout 5s` | Running time allowed for each pre-request or test script. Waiting on `bru.sleep` and `sendRequest` does not count |
| `--trace [request\|flow\|off]` | Send a W3C `traceparent` header with every request, for Dynatrace and other APM tools, and report each request's trace ID. `request` (the default with no value) gives each request its own trace, `flow` one trace per flow run; `off` sends none even if the file sets `trace` |
| `--junit FILE` | Also write a JUnit XML report. `run` writes one test case per step; `load` writes one per threshold |
| `--html FILE` | Also write a self-contained HTML report |
| `--format text\|json` | What goes to standard output |

Report paths are created along with any missing folders.

### Proxies

Requests go through the same proxy as the rest of the system. On Windows that is the proxy set in **Settings > Network & internet > Proxy** (Internet Options), with its bypass list; on macOS it is the one in System Settings. The `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` environment variables take precedence, in the desktop app and the CLI alike. A proxy that needs a user name and password takes them in its URL, like `HTTPS_PROXY=http://user:password@proxy.example.com:8080`.

`--proxy none` (in the desktop app, **No proxy** from the proxy button in the header, which shows the mode in use, or in a step's Details, **Connection** tab) sends every request straight to the server, whatever the variables and settings say, like Bruno with its proxy off. `--proxy URL` (**Proxy**, with its address) sends every request through one proxy. The app keeps its choice across launches.

A proxy set only by an automatic configuration script (a PAC file, "Use setup script" on Windows) is not read: set `HTTPS_PROXY` to the proxy that script picks for your API. Windows integrated sign-in to a proxy (NTLM or Kerberos) is not supported either.

## `run`

| Option | |
|---|---|
| `-v`, `--verbose` | Show passed checks, extracted values and script logs, not only failures |
| `--bail` | Stop at the first failed step; the rest are reported as skipped |

```
$ stormix run tests/api.stormix.yaml --env tests/staging.yaml
Log in, then read the profile

  ✓ Log in            POST    200    134 ms
  ✗ Read the profile  GET     401     12 ms
      ✗ status is 200: got 401

2 steps: 1 passed, 1 failed · 3/4 checks passed · 0.15 s
```

## `load`

Without overrides, `load` uses the file's `load` profile. A collection has no profile, so give it one with `--vus` or `--rate`.

| Option | |
|---|---|
| `--vus N` | Run N virtual users instead of the file's profile (with `--duration`, `--think-time`) |
| `--rate N` | Start N iterations a second instead of the file's profile (with `--duration`) |
| `--duration 1m` | How long to run with `--vus` or `--rate`, or a new length for a constant-rate or virtual-user profile |
| `--think-time 1s` | Pause between a virtual user's iterations |
| `--threshold RULE` | Add a threshold such as `"p95 < 300ms"`. Repeatable; adds to the file's |
| `--max-in-flight 1000` | Open models: the most iterations running at once; more are dropped and counted |
| `--grace 30s` | How long iterations still running at the end may take to finish |
| `--out FILE` | Also write the full report as JSON |
| `-q`, `--quiet` | No live progress line |

```
$ stormix load examples/demo-api.stormix.yaml
Demo API login · ramping from 10/s: 100/s over 5s, 100/s over 10s, 0/s over 5s (20s in all)

  requests    3050 (152.2/s), 0 failed (0.00%)
  iterations  1525, 0 failed
  checks      10675/10675 passed (100.00%)
  latency     min 0.1 ms  avg 0.4 ms  p50 0.3 ms  p90 0.5 ms  p95 0.5 ms  p99 0.9 ms  max 6.7 ms

  step              requests  failed        p50        p95        p99
  Log in                1525       0     0.4 ms     0.6 ms     0.9 ms
  Read the profile      1525       0     0.2 ms     0.4 ms     0.6 ms

  thresholds
    ✓ p95 < 50ms           0.5 ms
    ✓ errors < 1%          0.00%
    ✓ checks > 99%         100.00%
```

A request counts as failed when no response came back or a check on it failed. When nothing checks it, a 4xx or 5xx status also counts as failed. Latency is recorded in HDR histograms, so percentiles stay accurate to three significant digits at any volume.

## `diagnose`

`stormix diagnose FILE --step NAME` sends one step's request again and checks each stage on its way, to tell where it fails when it works from another client or another network:

1. **Proxy**: the proxy it goes through and where that setting came from, or none.
2. **Name lookup**: the server's addresses, or the proxy's.
3. **Connection**: a TCP connection to the server or the proxy.
4. **Proxy tunnel** (HTTPS through a proxy): what the proxy answered when asked to connect to the server, and who answered, like a `407` that wants a sign-in or a `403` from a web filter.
5. **TLS**: the certificate presented and who issued it. Under TLS inspection the issuer is your company's certificate authority, not a public one.
6. **Response**: the status, and who seems to have answered an error. Proxies, firewalls and CDNs such as Zscaler, FortiGate, Cloudflare, Akamai, AWS or a `Via` header are recognised from their headers and error page, and an HTML error page's title is shown.
7. **Without the proxy**: when the request went through a proxy and failed, it is sent once more straight to the server. If that works, the proxy is what refuses it, and the app offers **Use no proxy**. Diagnose then sends the request twice, which matters for a `POST` that changes data.

The enabled steps before it run first, for the variables they set, like a token. Its cache is ignored, so the request is always sent. `--format json` prints the whole diagnosis with the step's request and response. It exits with 0 when the request went through and its checks passed, 1 otherwise.

```sh
stormix diagnose orders.stormix.yaml --step "List orders" --env work
```

```text
Diagnosing `List orders`: GET https://api.example.com/orders

  ok   Earlier steps  Ran the step before it for the variables it sets.
       Proxy          Through http://proxy.corp.example:8080, from Windows proxy settings.
  ok   Name lookup    The proxy proxy.corp.example is at 10.20.0.15.
  ok   Connection     Connected to the proxy 10.20.0.15:8080. (2 ms)
  ok   Proxy tunnel   The proxy opened a connection to api.example.com:443. (41 ms)
  ok   TLS            Certificate for CN=api.example.com, issued by CN=Corp Inspection CA. (38 ms)
  FAIL Response       HTTP/1.1 403 Forbidden, from Zscaler, a corporate proxy: "Website blocked".

The 403 Forbidden seems to come from Zscaler, a corporate proxy, not from the API. ...
```

In the desktop app, **Diagnose** in a step's Details does the same, and the **Connection** tab shows the route every request took.

## `validate`

| Option | |
|---|---|
| `--strict` | Exit with 1 on warnings too, such as a `{{variable}}` that nothing defines |
| `--format json` | Print the parsed file and its issues as JSON |

## `import`

| Option | |
|---|---|
| `-o`, `--output FILE` | Write the test file here instead of printing it |
| `--force` | Replace the output file, and environment files, if they exist |

With `-o`, a collection's environments are written to `environments/NAME.yaml` next to the test file, so `stormix run FILE --env NAME` still finds them. Secrets have no value there; their names are listed under `$secrets`, and you pass them with `--var`.

What does not carry over is printed as a warning. See [Compatibility](COMPATIBILITY.md).

## `export`

| Option | |
|---|---|
| `-o`, `--output DIR` | The folder to write the collection into, or a `.yml`/`.yaml` file for a single bundled file |
| `--force` | Write into a folder that is not empty, or replace the bundled file |

Writes `FILE` as an [OpenCollection](COLLECTIONS.md#opencollection), the YAML format Bruno opens. `FILE` can be a test file, whose environments in `environments/` next to it go with it, or any collection Stormix reads, with its own environments. What Bruno has no place for is printed as a warning; see [Exporting to Bruno](COLLECTIONS.md#exporting-to-bruno).

## `init`

Writes an example test file, `stormix.yaml` by default. `--force` replaces an existing file.
