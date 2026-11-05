# Response cache

A step with its cache on is sent once. After that, runs and load test iterations **reuse its response** instead of sending the request again, until you clear the cache. It suits a login, or a lookup every iteration needs, that should not be part of the load.

## Turning it on

- **Flow tab**: the database icon on a step's card.
- **Steps tab**: **cache** on the step.
- **Load test setup**: the cache column next to each endpoint.
- **YAML**: `cache: true` on the step.

<Shot name="cache-load-setup" alt="Caching the login step in the load test setup" />

## How it behaves

- A response is kept per distinct request: the same method, URL, headers and body once variables are filled in. Change a variable the request uses and it is sent again.
- Only a response that **passed** is kept. A failed login is retried on the next run.
- Pre-request scripts still run first. Assertions, extraction and test scripts run on the reused response, so a token is still set.
- Results mark the steps that came from the cache as **cached**.

## In load tests

Cached steps become **prerequisites**: before the load starts, Stormix runs the flow once up to the last cached step, and every iteration then reuses those responses. That setup pass is reported on its own and stays out of the summary and thresholds. Reused responses are counted apart, not as requests, and stay out of the latency figures.

## Clearing it

The cache lives in memory for as long as the app is open (or one `stormix` command runs). Clear one step's cache from its card or the Steps tab, or the whole flow's with **Clear cache** in the Flow tab.

Stormix does not cache HTTP responses otherwise. More in the [test file reference](/reference/test-file#caching-a-step).
