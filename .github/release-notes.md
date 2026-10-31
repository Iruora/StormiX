Stormix 1.1 makes the desktop app easier to edit test files in and to debug failures with.

- **A real YAML editor.** Syntax errors and test-file mistakes (an unknown key, a wrong method, an undefined `{{variable}}`) are underlined as you type, with a hint and a jump to the next problem. Keys, methods, thresholds and variables autocomplete.
- **Details on failures.** Every step of a run, and each common failure of a load test, opens the failed checks, the request as sent, the response as received, and a **Copy as cURL** button. JSON reports from the CLI include the request and response of each failed step.
- **Variables and methods at a glance.** `{{variables}}` are coloured by where they come from, and hovering one shows its value, including what the last run extracted. Each HTTP method has its own colour.
- **Auto-update.** From this version on, the app tells you when a new release is out and installs it in one click. Updating from 1.0 to 1.1 is still a manual download, once.
- **Fixed:** switching a request body between json, raw and form no longer drops its content, such as a Bruno JSON body.

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
