# Running in CI

The `stormix` CLI runs the same test files and collections as the app, with no display needed. Its exit codes make CI jobs pass or fail:

| Exit code | Meaning |
|---|---|
| `0` | Everything passed |
| `1` | `run`: a check failed. `load`: a threshold failed |
| `2` | The file could not be read or run |

## Install the CLI

Each [release](https://github.com/Iruora/StormiX/releases/latest) has a `stormix-cli_<version>_<platform>` archive with a single binary. Download it and put `stormix` on the `PATH`.

## GitHub Actions

```yaml
jobs:
  api-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Stormix
        run: |
          curl -sL https://github.com/Iruora/StormiX/releases/download/v1.10.0/stormix-cli_1.10.0_x86_64-unknown-linux-gnu.tar.gz | tar -xz
          sudo mv stormix /usr/local/bin/
      - name: Regression run
        run: stormix run tests/api.stormix.yaml --env tests/ci.yaml --junit junit/api.xml --html reports/api.html
      - name: Load test with thresholds
        run: stormix load tests/api.stormix.yaml --env tests/ci.yaml --junit junit/load.xml --html reports/load.html
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: api-reports
          path: |
            junit/
            reports/
```

Collections work directly too, with no conversion step:

```sh
stormix run ./my-bruno-collection --env ci --junit junit/api.xml
stormix load my-api.postman_collection.json --vus 20 --duration 1m --threshold "p95 < 300ms"
```

## GitLab CI

```yaml
api-tests:
  image: ubuntu:24.04
  before_script:
    - apt-get update && apt-get install -y curl ca-certificates
    - curl -sL https://github.com/Iruora/StormiX/releases/download/v1.10.0/stormix-cli_1.10.0_x86_64-unknown-linux-gnu.tar.gz | tar -xz -C /usr/local/bin
  script:
    - stormix run tests/api.stormix.yaml --env tests/ci.yaml --junit junit.xml
  artifacts:
    when: always
    reports:
      junit: junit.xml
```

## Other CI systems

Most read JUnit XML into a test tab: Jenkins with the JUnit plugin, Azure Pipelines with `PublishTestResults`, CircleCI with `store_test_results`. In the JUnit output of `run`, each step is a test case; in that of `load`, each threshold is one.

## Secrets and per-environment values

Keep values that change between environments in an environment file and pass it with `--env`. Pass secrets from your CI's secret store with `--var`, so they are never written to the repository:

```sh
stormix run tests/api.stormix.yaml --env staging --var password="$API_PASSWORD"
```

See [Variables](/reference/test-file#variables) for the order in which values are filled in.

## Reports

| Option | Writes |
|---|---|
| `--junit FILE` | JUnit XML for the CI's test tab |
| `--html FILE` | A single HTML file with no scripts, to keep as an artifact or attach to a ticket |
| `--out FILE` | The full report as JSON |
| `--format json` | The JSON report on standard output instead of text |

## Proxies and tracing in CI

On a network that requires a proxy, `--proxy system|none|URL` chooses it ([Proxies](/features/proxies)). To find a slow CI request in your APM tool, add `--trace request` ([Tracing](/features/tracing)).
