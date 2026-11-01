Stormix 1.5.1 shows what the app is doing while slow actions run.

- **Progress while you wait:** a strip under the toolbar names what is under way, with a progress bar. A Bruno collection import counts its files as it reads them ("37 of 120 files"), and opening, saving, reopening a workspace, importing an environment and starting a load test show there too. **Run once** shows which step it is on.
- **Busy buttons:** the button you pressed shows a spinner, and buttons that would clash with it wait until it finishes. Saving shows **Saved**. Exporting reports, deleting runs and downloading updates show progress as well.
- **No more freezes:** importing a large collection, and reading or saving files, no longer freezes the window.

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
