Stormix 1.6.0 supports GraphQL natively.

- **GraphQL steps:** a new `graphql` body with `query`, `variables` and `operation_name`, plus a GraphQL editor in the Steps tab. A `{{variable}}` is sent as the type the query declares, so `id: "{{user_id}}"` goes out as a number for `$id: Int!`.
- **GraphQL errors fail the step:** a response with an `errors` array fails the step even with status 200, and load tests count it as an error. Set `allow_errors: true` for a step that expects errors.
- **Imports fixed:** GraphQL requests from Postman and Bruno whose variables use bare `{{variables}}`, like `{ "id": {{userId}} }`, were sent with `null` variables. They now keep their variables and fill them in when sent.

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
