# Import and export collections

Stormix reads the collections you already have and writes them back out, so you can move between Stormix, Postman and Bruno without retyping a request.

## Importing

In the app, **File › Import a collection**:

| Source | Pick |
|---|---|
| **Postman** | A collection export (`.json`, v2.0 or v2.1) |
| **Bruno** | A collection folder of `.bru` files, or an OpenCollection folder |
| **OpenCollection** | An `opencollection.yml`, or a bundled `.yml` file |

The collection becomes a flow: each request a step, folders as `Folder / Request` names, and its collection and folder headers, auth, variables and scripts applied to each request under them. Environments come along ([Environments](/app/environments)). The **Problems** tab lists anything that did not carry over; nothing is dropped silently.

<Shot name="oc-import" alt="An imported OpenCollection" />

On the command line, collections run directly, or `stormix import` turns one into a test file:

```sh
stormix run ./my-bruno-collection --env local
stormix import ./my-bruno-collection -o my-api.stormix.yaml   # also writes environments/local.yaml
```

## Exporting

**File › Export as** writes the open test file and its environments as:

- a **Bruno folder** (OpenCollection), which Bruno opens;
- one bundled **OpenCollection file**;
- a **Postman collection** (v2.1), with each environment as a `.postman_environment.json` file next to it.

<Shot name="export-as" alt="File › Export as" />

Checks become Bruno assertions or `pm.test` lines, and extractions become post-response variables or collection variables. What the target has no place for is listed when the export ends.

<Shot name="oc-export" alt="A test file exported to Bruno" />

```sh
stormix export my-api.stormix.yaml -o ./for-bruno   # a folder Bruno opens
stormix export my-api.stormix.yaml -o my-api.yml    # one bundled OpenCollection file
stormix export my-api.stormix.yaml -o my-api.json   # a Postman collection
```

## What carries over

[Collections](/reference/collections) details import and export, and [Compatibility](/reference/compatibility) lists bodies, auth types and script APIs feature by feature.
