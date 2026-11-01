# CLI reference

`stormix` runs the same files as the desktop app, for CI and quick runs from a terminal. Every command takes `--help`.

```sh
stormix init [FILE]                 # write an example test file (stormix.yaml)
stormix validate FILE               # check a file without sending anything
stormix run FILE                    # run the flow once, as a functional test
stormix load FILE                   # run it under load and check thresholds
stormix import COLLECTION -o FILE   # turn a Postman or Bruno collection into a test file
```

`FILE` is a test file, a Postman collection export (`.json`) or a Bruno collection folder, for `validate`, `run` and `load` alike.

## Exit codes

| Code | `validate` | `run` | `load` |
|---|---|---|---|
| 0 | No errors | Every check passed | Every threshold held |
| 1 | Errors (and warnings with `--strict`) | A step or check failed | A threshold failed |
| 2 | The file could not be read | The run could not start: unreadable file, errors in it, bad options | The same |

## Options for `run` and `load`

| Option | |
|---|---|
| `--var NAME=VALUE` | Set a variable, overriding the file and the environment. Repeatable |
| `--env FILE\|NAME` | Variables from a YAML or JSON map, a Postman environment export or a Bruno `.bru` environment. For a Bruno collection, an environment name works too |
| `--timeout 30s` | Time allowed for each request |
| `--insecure` | Accept invalid or self-signed TLS certificates |
| `--no-follow-redirects` | Report redirects instead of following them |
| `--script-timeout 5s` | Running time allowed for each pre-request or test script. Waiting on `bru.sleep` and `sendRequest` does not count |
| `--junit FILE` | Also write a JUnit XML report. `run` writes one test case per step; `load` writes one per threshold |
| `--html FILE` | Also write a self-contained HTML report |
| `--format text\|json` | What goes to standard output |

Report paths are created along with any missing folders.

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

## `validate`

| Option | |
|---|---|
| `--strict` | Exit with 1 on warnings too, such as a `{{variable}}` that nothing defines |
| `--format json` | Print the parsed file and its issues as JSON |

## `import`

| Option | |
|---|---|
| `-o`, `--output FILE` | Write the test file here instead of printing it |
| `--force` | Replace the output file if it exists |

What does not carry over is printed as a warning. See [Compatibility](COMPATIBILITY.md).

## `init`

Writes an example test file, `stormix.yaml` by default. `--force` replaces an existing file.
