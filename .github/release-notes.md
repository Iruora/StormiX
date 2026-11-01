Stormix 1.4.0 adds a per-step response cache and makes the desktop app follow the system text size.

- **Response cache:** mark a step with `cache: true`, or use the cache switch in the Flow tab, the Steps tab and the load test setup, to send it once and reuse its response. In a load test, cached steps such as a login run once before the load and stay out of the metrics. You can clear one step's cache or the whole flow's at any time.
- **Large text and scaling:** the app scales evenly with the system text size and display scaling. The header and toolbars wrap instead of hiding **Run once** and **Load test**, and the results panel keeps sensible minimum widths when resized.
- The Flow tab has an **All steps** switch that turns every step on or off.

If you are on 1.1 or later, the app offers this update by itself. On 1.0, download it once from the table below.

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
