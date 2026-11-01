Stormix 1.3.0 gives you control over which steps run, in the flow and in load tests.

- **Flow tab**: the steps as a diagram. Turn steps off, drag them to reorder, or move them to a **Set aside** tray without deleting them. Each step shows its last result and warns when a variable it needs comes from a step that is off, set aside or now runs after it.
- **Load test setup**: **Load test** opens a dialog to pick which endpoints to send, with a warning when a chosen step needs a variable only a left-out step provides. The load profile and thresholds are edited there too, and a file without a `load` section can now be load tested.
- **Results table**: a load test's per-step table sorts by any column and filters by name, failing steps and minimum p95.
- In the test file, `enabled: false` turns a step off, and `flow.set_aside` holds steps kept out of the flow.

If you are on 1.1 or later, the app offers this update by itself. On 1.0, download it once from the table below.

See the [changelog](https://github.com/Iruora/StormiX/blob/main/CHANGELOG.md) for details and [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md) to begin.

## Install

| System | File |
|---|---|
| macOS (Apple silicon) | `Stormix_<version>_aarch64.dmg` (or `.app.zip`) |
| macOS (Intel) | `Stormix_<version>_x64.dmg` (or `.app.zip`) |
| Windows | `Stormix_<version>_x64-setup.exe` (or the `.msi`), or `_x64_portable.zip` to run without installing |
| Linux | `.AppImage`, `.deb` (Debian, Ubuntu) or `.rpm` (Fedora) |
| CLI only | `stormix-cli_<version>_<target>.tar.gz` (`.zip` on Windows) |

The builds are not code signed yet, so macOS and Windows ask you to confirm once before the first launch. [Getting started](https://github.com/Iruora/StormiX/blob/main/docs/GETTING_STARTED.md#install) has the steps.
