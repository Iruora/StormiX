Stormix 1.7.0 adds request tracing and Bruno's OpenCollection format.

- **Trace requests in Dynatrace and other APM tools:** with `trace: request` (or `trace: flow`), every request carries a W3C `traceparent` header and every result records its trace ID, so a slow or failed request can be followed through each service it reached. Details shows the trace ID with a copy button, and load test results list the slowest traced requests.
- **OpenCollection import:** Bruno collections saved in OpenCollection YAML, Bruno's format since v3, run and import like `.bru` ones: a folder with `opencollection.yml`, that file, or a single bundled `.yml`, environments included.
- **Export to Bruno:** `stormix export FILE -o DIR` and the app's **Export to Bruno** button write a test file as an OpenCollection that Bruno opens. Checks become Bruno assertions, and anything Bruno has no place for is listed.

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
