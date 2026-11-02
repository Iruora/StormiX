Stormix 1.8.0 helps when a request fails on one network but works elsewhere.

- **Requests use the system proxy:** the app and the CLI now send requests through the proxy set in Windows or macOS settings, as browsers and Bruno do, instead of connecting directly. On a corporate network a direct connection can be refused, often with a 403. `HTTPS_PROXY` and `NO_PROXY` still take precedence.
- **Diagnose where a request fails:** **Diagnose** in a step's Details, and `stormix diagnose FILE --step NAME`, send the request again and check each stage: the proxy, name lookup, the connection, the proxy's answer, the TLS certificate and who issued it, and the response. An error is matched to whoever seems to have answered it, like a corporate proxy, a firewall, a CDN or the API. **Copy report** copies the result for a ticket.
- **Connection tab:** Details shows the route each request took: its proxy, the address it connected to, the HTTP version and the server's certificate.

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
