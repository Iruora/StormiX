Stormix 1.2.0 makes room in the app and makes environments easier to work with.

- **More room**: the sidebar is now a column of icons, and the results panel can be hidden or resized by dragging its edge.
- **Import env**: load an environment from a YAML or JSON file, a Postman environment export or a Bruno `.bru` file, with or without a collection.
- **Secrets**: fill in an environment's secrets (Bruno's `vars:secret`) before a run. The values stay in memory only.
- Environments stay when you open another file. Opening a test file also loads the `environments/` folder next to it, which is where `stormix import -o` now writes a collection's environments, so `--env NAME` keeps working on the YAML.
- Script retry loops: waiting on `bru.sleep` or `sendRequest` no longer counts toward the script time limit.

If you are on 1.1, the app offers this update by itself. On 1.0, download it once from the table below.

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
