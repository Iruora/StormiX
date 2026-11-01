Stormix 1.5.0 brings back what you had open when the app restarts, and lets you see and change the loaded environment.

- **Workspaces:** what you have open is saved as you work: the test file (and unsaved edits), the environments and the chosen one, the editor tab and the steps left out of the last load test. The app opens the last workspace on launch instead of the example. The new **Workspaces** page lists them to open, rename or delete, and **New workspace** starts another. Secret values are never saved.
- **Variables:** next to the environment picker, see the chosen environment's variables and change them for this session only, or save them to the environment's file (`.bru`, Postman, JSON or YAML, in its own format). Secrets stay masked and are never written with a value.

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
