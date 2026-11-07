**Release candidate for Stormix 1.11.0.** Try it before 1.11.0 ships: it lets you stop a load test, replay it, and watch the API's CPU, memory and health while it runs.

- **Stop and Replay load tests.** **Stop** ends a running test and keeps what was measured so far (Ctrl+C in the CLI). **Replay** runs a finished test again with the same profile and thresholds.
- **API CPU and memory during a load test**, read from Spring Boot Actuator, Prometheus `/metrics` endpoints (Quarkus, prom-client, prometheus-net, Go) or Dynatrace, shown live, in the results, the HTML report and the run history.
- **Health of the API during the test**: when it went `DOWN` or stopped answering, what was down, and when it came back.
- **Fix:** a virtual-user load test longer than 30 seconds no longer stops early.
- **Documentation website** at [iruora.github.io/StormiX](https://iruora.github.io/StormiX/).

This is a pre-release: the app's automatic update does not offer it, and stays on 1.10.0 until 1.11.0 is published. Download it from the table below to try it; once 1.11.0 is out, the app offers that update by itself.

See the [changelog](https://github.com/Iruora/StormiX/blob/main/CHANGELOG.md) for details and [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md) to begin.

## Install

| System | File |
|---|---|
| macOS (Apple silicon) | `Stormix_<version>_aarch64.dmg` (or `.app.zip`) |
| macOS (Intel) | `Stormix_<version>_x64.dmg` (or `.app.zip`) |
| Windows | `Stormix_<version>_x64-setup.exe` (or the `.msi`), `_x64_installer.zip` (both installers in one zip), or `_x64_portable.zip` to run without installing |
| Linux | `.AppImage` (a release candidate has no `.deb` or `.rpm`, so that the final 1.11.0 packages install over it cleanly) |
| CLI only | `stormix-cli_<version>_<target>.tar.gz` (`.zip` on Windows) |

The builds are not code signed yet, so macOS and Windows ask you to confirm once before the first launch. [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md#install) has the steps.
