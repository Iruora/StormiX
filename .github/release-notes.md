Stormix 1.9.0 tidies the header into menus, lists the steps beside the editor, and exports to Postman.

- **File menu**: New, Open, Save, Save as, every import and every export in one place, with Ctrl O, Ctrl S and Ctrl Shift S.
- **Environment menu**: one button with the environment in use, its changed variables and missing secrets; the list, Variables, Secrets, Import and Remove inside.
- **Ctrl K search** for every action, environment, proxy mode, step and workspace.
- **Settings** at the bottom of the sidebar, with the proxy and the keyboard shortcuts.
- **Steps listed beside the editor**, like Bruno: click a step to edit it; **Flow settings** holds the name, tracing, load profile and thresholds.
- **Results panel tabs**: Problems, Last run and Summary.
- **Export to Postman and OpenCollection**: **File › Export as** writes a Bruno folder, one OpenCollection file, or a Postman collection with its environments. `stormix export FILE -o NAME.json` does the same from the command line.

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
