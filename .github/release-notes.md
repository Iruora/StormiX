Stormix 1.11.0 tells you how far your API goes and how many pods it needs, and lets you watch its CPU, memory and health during a load test.

- **Find the limit.** A `find_limit` load profile raises the rate step by step, checks each step against the thresholds on its own, and stops at the first step that breaks one. The last step that held is what one instance sustains.
- **`cpu` threshold**, like `cpu < 85%`, so a step can also break on a CPU ceiling.
- **Pods needed.** Give the production peak, growth, target CPU and pod memory limit, and the result works out how many pods you need for CPU and for memory, compared with the pods you run today, plus the requests in flight per pod for sizing thread and connection pools.
- **Stop and Replay load tests.** **Stop** ends a running test and keeps what was measured so far (Ctrl+C in the CLI). **Replay** runs a finished test again with the same profile and thresholds.
- **API CPU and memory during a load test**, read from Spring Boot Actuator, Prometheus `/metrics` endpoints (Quarkus, prom-client, prometheus-net, Go) or Dynatrace, shown live, in the results, the HTML report and the run history.
- **Health of the API during the test**: when it went `DOWN` or stopped answering, what was down, and when it came back.
- **Fixes:** a virtual-user load test longer than 30 seconds no longer stops early, and editing the flow while a load test runs no longer hides it or lets a second one start.
- **Documentation website** at [iruora.github.io/StormiX](https://iruora.github.io/StormiX/).

If you are on 1.1 or later, the app offers this update by itself. On 1.0, download it once from the table below. If you tried 1.11.0-rc.1 on Linux, delete its AppImage when you switch to the `.deb` or `.rpm`.

See the [changelog](https://github.com/Iruora/StormiX/blob/main/CHANGELOG.md) for details and [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md) to begin.

## Install

| System | File |
|---|---|
| macOS (Apple silicon) | `Stormix_<version>_aarch64.dmg` (or `.app.zip`) |
| macOS (Intel) | `Stormix_<version>_x64.dmg` (or `.app.zip`) |
| Windows | `Stormix_<version>_x64-setup.exe` (or the `.msi`), `_x64_installer.zip` (both installers in one zip), or `_x64_portable.zip` to run without installing |
| Linux | `.AppImage`, `.deb` (Debian, Ubuntu) or `.rpm` (Fedora) |
| CLI only | `stormix-cli_<version>_<target>.tar.gz` (`.zip` on Windows) |

The builds are not code signed yet, so macOS and Windows ask you to confirm once before the first launch. [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md#install) has the steps.
