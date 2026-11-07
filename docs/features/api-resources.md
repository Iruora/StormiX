# API CPU and memory

A load test shows what the clients see: latency, throughput and errors. To see what that load does to the API itself, Stormix can read the API's own metrics endpoint during the test. The API's **CPU** and **memory** then show beside the latency, live while the test runs and in the results, reports and run history.

Stormix reads the endpoint once just before the load starts, which gives you a baseline, and then every second until the test ends.

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
  - actuator:
      url: "{{billing_url}}/actuator"   # reads /actuator/prometheus if exposed, else /actuator/metrics
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

If a read fails, for example with `401 Unauthorized`, `404 Not Found` or a timeout, the load test carries on. The results say how many reads failed and why the last one did.

## Things to know

- The figures are what the process reports about itself. A JVM's heap doesn't show a container's memory limit, and for a service with several replicas each endpoint is one instance. Add one `monitor` entry per instance that you want to watch.
- Reads go through the same proxy setting as the test's requests.
- In reports, a URL is shown without its query string or password, in case they hold a token.
- APIs that can't expose an endpoint will be able to use Dynatrace as a source in a later version.
