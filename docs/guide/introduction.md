# What is Stormix?

Stormix tests HTTP APIs in two ways with one engine:

- **Regression runs.** A *flow* is a list of requests, run in order. Each step can pull values out of its response into variables for the next steps, and check the status, headers, JSON body or timing. Run it once and you know whether the API still behaves.
- **Load and stress tests.** The same flow, replayed by many users at once, under a load model you choose. You get exact latency percentiles, error rates and throughput per step, and thresholds that pass or fail the test.

You write flows as YAML test files, or you bring the collections you already have: Postman, Bruno (`.bru` or OpenCollection) and their environments run as they are, scripts included.

<Shot name="app-last-run" alt="The desktop app after a passing run" />

## Two ways to use it

| | Desktop app | `stormix` CLI |
|---|---|---|
| For | Building and debugging flows, exploring results | CI pipelines, scripts, quick runs |
| Runs | **Run once**, **Load test** with a live chart | `stormix run`, `stormix load` |
| Results | Run history, diffs between runs, request details | Text, JSON, JUnit XML, HTML |
| Platforms | macOS, Windows, Linux installers | Single binary for each platform |

Both use the same engine and read the same files, so a flow you build in the app runs unchanged in CI.

## How it compares

Stormix aims at the space between an API client and a load testing tool:

- **Like k6**, it runs open and closed load models, measures latency with HDR histograms and gates CI with thresholds. Unlike k6, tests are YAML or existing collections rather than JavaScript programs, and there is a desktop app to build and inspect them. On the same machine it sent about twice k6's throughput with a tenth of its memory ([how this was measured](/guide/load-testing#how-fast-is-it)).
- **Like Postman and Bruno**, it runs chained requests with variables, environments and `pm.*` / `bru.*` scripts. Unlike them, the same flow can be put under load, every run is kept to compare with the next one, and failures can be diagnosed down to the proxy and TLS stage.

[Compatibility](/reference/compatibility) lists what carries over from Postman, Bruno and k6, feature by feature.

## What it is made of

Stormix is written in Rust: a `reqwest` and `rustls` HTTP client on `tokio`, scripts in a sandboxed QuickJS engine, run history in SQLite. The desktop app is a Tauri shell with a React interface. Everything stays on your machine: there is no account and nothing is sent anywhere except the requests you run, and the app's check for updates against GitHub releases.

It is free and open source under the MIT or Apache 2.0 license.

## Next

- [Getting started](/guide/getting-started): install, a first run, a first load test and CI, in ten minutes.
- [A tour of the app](/app/tour): where everything is.
- [Test files](/reference/test-file): the YAML format.
