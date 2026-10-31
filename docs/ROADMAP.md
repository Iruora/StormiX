# Roadmap

Each milestone ends with something runnable. The desktop app (M4) can start in parallel once M1 lands.

| Milestone | What ships | Done when |
|---|---|---|
| **M0 Foundations** (done) | Cargo workspace, Tauri shell, CI and release, flow model and YAML test file format, `stormix validate` | CI builds installers for macOS, Windows and Linux |
| **M1 Flow runner and CLI** (done) | HTTP/1.1 and HTTP/2 client, variables, extraction, assertions, `stormix run`, JUnit output | A hand-written flow runs green in CI |
| **M2 Collections** (done) | Postman v2.1 and Bruno import, pre-request and test scripts in embedded QuickJS (`pm.*`, `bru.*`) | Fixture collections run unchanged |
| **M3 Load engine** (done) | Constant rate, ramping and virtual-user models, HDR latency histograms, thresholds, `stormix load` | Benchmarked against k6 on the same machine |
| **M4 Desktop app** | Import, flow editor, live load charts, run history, regression diffs | A QA engineer goes from import to report without leaving the app |
| **M5 v1.0** | HTML report, docs, compatibility table, signed-off release | v1.0 tagged with installers |

Later: gRPC and WebSocket, Insomnia, OpenAPI and HAR import, data-driven runs, distributed load, Prometheus and OpenTelemetry output.

## Planned crates

`flow` (M0), `engine` and `cli run` (M1), `cli load` (M3), `collections` and `script` (M2, see [Collections](COLLECTIONS.md)), `metrics` (M3), `report` (M5), `store` (M4).
