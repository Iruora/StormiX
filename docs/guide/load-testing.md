# Load testing

A load test runs your flow many times at once and measures how the API holds up. In Stormix the flow is the same one you run as a regression test, so you check that it passes first, then add load.

## Choosing a load model

There are three, and the choice decides what a slow API does to the test.

| Model | You set | What happens when the API slows down |
|---|---|---|
| **Constant rate** (`constant_rate`) | iterations a second, duration | Nothing changes: new iterations keep starting on schedule, so requests queue up, as real traffic would |
| **Ramping rate** (`ramping`) | a start rate and stages, each a target rate and a duration | Same as constant rate, with the rate moving linearly between stages |
| **Virtual users** (`virtual_users`) | number of users, duration, think time | Each user waits for its iteration to finish before the next, so a slower API gets fewer requests |

The first two are *open* models and the third a *closed* one. To find the rate at which an API breaks, ramp an open model up in stages. To reproduce a known number of concurrent users clicking through an app, use virtual users with a think time.

```yaml
load:
  ramping:
    start_rate: 10
    stages:
      - { duration: 1m, target: 100 }   # ramp up
      - { duration: 5m, target: 100 }   # hold
      - { duration: 30s, target: 0 }    # ramp down
```

In open models, Stormix caps the iterations running at once at 1000 (`--max-in-flight`). When the API is so slow that more would be due, the extra iterations are **dropped and reported**, rather than silently lowering the rate you asked for.

## Thresholds

Thresholds turn a load test into a pass or fail result:

```yaml
thresholds:
  - p95 < 300ms     # 95% of requests answered within 300 ms
  - errors < 1%     # under 1% of requests failed
  - checks > 99%    # over 99% of assertions passed
  - rps > 200       # at least 200 requests a second over the run
```

When one fails, the app marks it and `stormix load` exits with 1. See [Thresholds](/reference/test-file#thresholds) for every metric.

## What counts as a failure

A request fails when no response came back, when a check on it failed, or, when nothing checks it, when its status is 4xx or 5xx. A GraphQL step also fails on GraphQL `errors` in a 200 response. Script errors count as failed checks.

## Reading the results

- **Latency**: min, average, p50, p90, p95, p99 and max for the whole test and for each step. Percentiles come from HDR histograms and are exact to three significant digits at any volume.
- **Throughput**: requests and iterations a second, and how many iterations were dropped.
- **Per step**: requests, failures and percentiles for each step, so you can see which endpoint is slow.
- **Most common failures**: the failures grouped by step and reason, with the first request and response of each kept for inspection.
- **Timeline**: requests, failures and p95 for each second of the test.
- **Slowest traced requests**, when [tracing](/features/tracing) is on: the trace IDs to look up in your APM tool.

## Prerequisite steps

Steps that only need to run once, such as a login, can be [cached](/features/cache): they run before the load starts, and their responses are reused by every iteration without counting in the metrics.

## How fast is it?

`bench/compare.sh` runs the same flow against the same demo API with Stormix and with k6, and CI runs it on each change. In the run published with 1.0, at 50 virtual users on the same machine, Stormix sent **42,700 requests a second using 13 MB of memory**, against k6's 22,300 using 153 MB. The point is not the absolute number, which depends on the machine, but that the load generator is unlikely to be your bottleneck.

## Next

- [Load tests in the app](/app/load-tests): the setup dialog, live chart and results table.
- [`stormix load`](/reference/cli#load): every option on the command line.
