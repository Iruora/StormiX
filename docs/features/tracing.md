# Tracing in Dynatrace and APM tools

When a request is slow or fails under load, the API's own monitoring knows why. Tracing links the two: with it on, every request Stormix sends carries a W3C `traceparent` header, and every result records its **trace ID**. Search for that ID in Dynatrace, Datadog, New Relic, Elastic, Jaeger or Grafana Tempo to follow the request through every service it reached.

## Turning it on

- **Steps tab › Flow settings › Trace requests**, or the same setting in the load test setup: **Off**, **Each request** or **Each flow run**.
- **YAML**: `trace: request` or `trace: flow` on the flow.
- **CLI**: `--trace request|flow|off` on `run` and `load`.

<Shot name="trace-setting" alt="Trace requests in Flow settings" />

**Each request** gives every request its own trace: one trace ID, one request. **Each flow run** makes each run of the flow, or each load test iteration, one trace with a span per request, so a whole journey (log in, add to cart, pay) shows as one trace.

## Finding a request

**Details** shows the request's trace ID with **Copy trace ID**.

<Shot name="trace-details" alt="A trace ID in Details" />

Load test results list the **slowest traced requests** of each step, with a copy button for each. The JSON and HTML reports carry them too.

<Shot name="trace-slowest" alt="The slowest traced requests of a load test" />

## In Dynatrace

OneAgent reads `traceparent` when W3C Trace Context is turned on (the default in recent versions). Search for the trace ID under **Distributed traces**. The services behind the API must be monitored for their part to show; Stormix starts the trace but reports nothing to the APM tool itself.

More in the [test file reference](/reference/test-file#tracing-requests).
