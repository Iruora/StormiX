# API CPU, memory and health

A load test shows what the clients see: latency, throughput and errors. To see what that load does to the API itself, Stormix can read the API's own metrics endpoint during the test. The API's **CPU** and **memory** then show beside the latency, live while the test runs and in the results, reports and run history. With a health check, such as `/actuator/health`, Stormix also spots the API **going unhealthy**: when it went down, what was down, and when it came back.

Stormix reads the endpoint once before the load starts, and waits for that read (up to 10 seconds) so it gives you a true baseline. It then reads it every second until the test ends.

## What the API needs

A metrics endpoint that Stormix can reach. Most stacks already have one, or can add one with a single dependency:

| Stack | Endpoint | How to turn it on |
|---|---|---|
| Spring Boot | `/actuator/prometheus` | Add `micrometer-registry-prometheus` and `management.endpoints.web.exposure.include=health,prometheus` |
| Spring Boot without that registry | `/actuator/metrics` | `management.endpoints.web.exposure.include=health,metrics`. Choose **Spring Boot Actuator** and give the `/actuator` base URL |
| Quarkus | `/q/metrics` | `quarkus-micrometer-registry-prometheus` |
| Micronaut | `/prometheus` | `micronaut-micrometer-registry-prometheus` |
| .NET | `/metrics` | `prometheus-net.AspNetCore` (`app.MapMetrics()`), or OpenTelemetry's Prometheus exporter |
| Node.js | `/metrics` | `prom-client` with `collectDefaultMetrics()` |
| Go | `/metrics` | `client_golang`'s `promhttp.Handler()` |

Management endpoints are often on their own port or behind authentication. Stormix can send headers, such as a bearer token, with each read.

## Setting it up

- **Load test setup › API CPU and memory**: add a metrics endpoint, pick **Prometheus text** or **Spring Boot Actuator**, and paste its URL. URLs can use `{{variables}}`. It is saved into the test file.
- **YAML**: a `monitor` list at the top of the file. See below.
- **CLI**: `stormix load FILE --monitor URL`. A URL that ends in `/actuator` is read as Spring Boot Actuator. Any other URL is read as Prometheus text.

```yaml
monitor:
  - prometheus:
      url: "{{base_url}}/actuator/prometheus"
      name: orders-api                 # optional, the URL's host by default
      headers:                          # optional
        - { name: Authorization, value: "Bearer {{actuator_token}}" }
      interval: 1s                      # optional, 1s by default
      health: "{{base_url}}/actuator/health"   # optional, any health check URL
  - actuator:
      url: "{{billing_url}}/actuator"   # reads /actuator/prometheus if exposed, else /actuator/metrics,
                                        # and /actuator/health without being asked
```

Keep the token in an environment's `$secrets` so it is never written to a file.

## What is shown

| Figure | Read from |
|---|---|
| **CPU**, where 100% is one full core, as `top` shows it | `process_cpu_seconds_total` (or Micrometer's `process_cpu_time_ns_total`) between two reads; otherwise Micrometer's `process_cpu_usage` × `system_cpu_count` |
| **Memory** | `process_resident_memory_bytes` (resident memory, RSS) when the endpoint has it. Otherwise, for the JVM, heap plus non-heap in use (`jvm_memory_used_bytes`) |
| **Heap** (JVM) | Heap in use, and the maximum heap (`jvm_memory_max_bytes`) |
| **GC pauses** (JVM, Node) | Milliseconds of garbage collection pause per second (`jvm_gc_pause_seconds_sum`, `nodejs_gc_duration_seconds_sum`) |

The results show the average and peak CPU, and the memory before the load, at its peak and at the end. A clear rise from the first value to the last can point to a leak. Both CPU and memory are drawn over the same time axis as the throughput chart, so a spike in latency can be matched to a spike in CPU or a GC storm.

## Health

Each read also reads the health check, when there is one: `health:` on any entry, and `/health` under the URL for Spring Boot Actuator without being asked. The API counts as **unhealthy** while the check says `DOWN` or `OUT_OF_SERVICE`, answers with an error status (like `HTTP 503`), or does not answer at all (`UNREACHABLE`, for example when it crashed or stalled). `UP` and `UNKNOWN` count as healthy.

For Spring Boot, the components that are not up are named, like `db: DOWN` or `redis.primary: DOWN`. That needs `management.endpoint.health.show-components=always` (or `show-details`); without it, only the overall status is shown.

The results say **Healthy throughout**, or list each time the API went unhealthy: `DOWN at 12.0 s (db: DOWN), healthy again at 16.0 s, after 4.0 s`. The result's header shows **API went unhealthy**, and the CPU and memory chart shades those stretches red. While the test runs, the live panel shows the status as soon as it changes, and the CLI's live line names any API that is down. A health check that answers 401, 403 or 404 on its first read is left alone, and the results say why, so an API that doesn't expose it is never shown as unhealthy.

If a read fails, for example with `401 Unauthorized`, `404 Not Found` or a timeout, the load test carries on. The results say how many reads failed and why the last one did.

## Things to know

- The figures are what the process reports about itself. A JVM's heap doesn't show a container's memory limit, and for a service with several replicas each endpoint is one instance. Add one `monitor` entry per instance that you want to watch.
- Reads go through the same proxy setting as the test's requests.
- In reports, a URL is shown without its query string or password, in case they hold a token.
- APIs that can't expose an endpoint will be able to use Dynatrace as a source in a later version.
