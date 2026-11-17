Stormix 1.12.0 runs flow steps in parallel, stops a flow at its first failure, fills load tests with test data, copies any step as cURL, and can explain a failure with your own Claude Code or Copilot CLI.

**Changes what existing files do:** a flow now stops at its first failed step and skips the rest, instead of sending them without the token or ID the failed step was to give them. Add `on_failure: continue` to a file to keep the old behaviour, or `continue_on_failure: true` on one step to let just that step fail.

- **Parallel branches.** `after:` on a step names the steps it waits for: steps after the same step run at the same time, and a step after several waits for all of them. The Flow tab draws the branches side by side, and **Branch** on a card adds one. A failed step stops only its own branch. Files without `after:` run as before.
- **Stop at a failed step.** Skipped steps are marked in the run result, the Flow tab and the Steps tab, and counted per step in load test results. **When a step fails** in the Flow tab header switches a flow back to running every step.
- **Test data for load tests.** A `data` section (**Test data** in the load test setup) holds lists of values, typed in, imported from CSV or linked as a file, that fill `{{variables}}` differently for each iteration or each virtual user, picked at random, in order or each once.
- **Copy as cURL** on every step, in the Flow tab and Steps, without running first, with variables and secrets filled in, for bash or Windows cmd.
- **Explain with your own AI.** **Explain** on a failed step or a load test result asks Claude Code or GitHub Copilot CLI, whichever is installed and signed in on your computer. Stormix needs no API key; secrets and tokens are hidden first, and **Show what is sent** shows the exact text. Also `stormix explain RESULT.json` in the CLI.

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
