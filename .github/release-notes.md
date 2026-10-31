Stormix 1.0 is the first stable release: API regression runs and load tests from one engine, in a desktop app and the `stormix` CLI. Postman and Bruno collections run as they are, scripts included.

- Desktop app: import collections, edit flows as steps or YAML, run once or under load with a live chart, keep every run, and compare two runs to see what broke.
- CLI for CI: `stormix run` and `stormix load` with JUnit XML, JSON and self-contained HTML reports, and thresholds that set the exit code.
- Load engine: constant-rate, ramping and virtual-user models with HDR latency percentiles. 42.7k req/s against k6's 22.3k at 50 VUs on the same machine, in 13 MB of memory.

See the [changelog](https://github.com/Iruora/StormiX/blob/main/CHANGELOG.md) for everything in this release and [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md) to begin.

## Install

| System | File |
|---|---|
| macOS (Apple silicon) | `Stormix_<version>_aarch64.dmg` (or `.app.zip`) |
| macOS (Intel) | `Stormix_<version>_x64.dmg` (or `.app.zip`) |
| Windows | `Stormix_<version>_x64-setup.exe` (or the `.msi`), or `_x64_portable.zip` to run without installing |
| Linux | `.AppImage`, `.deb` (Debian, Ubuntu) or `.rpm` (Fedora) |
| CLI only | `stormix-cli_<version>_<target>.tar.gz` (`.zip` on Windows) |

The builds are not code signed yet, so macOS and Windows ask you to confirm once before the first launch. [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md#install) has the steps.
