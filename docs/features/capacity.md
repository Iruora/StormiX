# Capacity and pods

How many pods (or containers, or servers) does the API need for its busiest hour? Stormix answers in two steps: it finds how much **one pod** sustains, then divides your production peak by that, with room to spare.

## 1. Find one pod's limit

The `find_limit` load profile raises the rate step by step. Each step is measured on its own and checked against your [thresholds](/reference/test-file#thresholds). The test ends at the first step that breaks one, and the last step that held is the limit.

```yaml
load:
  find_limit:
    start_rate: 10      # iterations per second of the first step
    increase: 10        # added at each step
    every: 30s          # how long each step lasts
    max_rate: 300       # the highest rate tried
thresholds: [p95 < 300ms, errors < 1%, cpu < 85%]
monitor:
  - actuator: { url: "{{base_url}}/actuator" }
```

Which thresholds to use is your service level: the latency and error rate you promise, plus a CPU ceiling. A `cpu` threshold reads the API's CPU from a [monitor](/features/api-resources), so add one: it also makes the sizing below much better.

Tips for a limit you can trust:

- Test **one pod**, or tell Stormix how many served the test (`tested_pods`). Give the pod the same CPU and memory limits as in production, like `docker run --cpus=1` or the same Kubernetes `resources.limits`.
- Make steps long enough for latency and CPU to settle: 30 s to 1 min. With Dynatrace (one reading a minute), use 2 min or more.
- Use a realistic mix of requests. The limit is in requests per second of *this* flow; a flow of only cheap requests overstates it.
- Run Stormix from a machine that is not the bottleneck itself.

In the app, pick **Find the limit** under **Load profile** in the load test setup. On the command line, `stormix load FILE` prints each step as it ends.

## 2. Size the pods

Add a `capacity` section with your production peak, or fill in **Size pods for** in the load test setup:

```yaml
capacity:
  peak_rps: 600        # requests/s at the busiest time, all pods together
  growth: 30%          # growth to plan for (default 30%)
  target_cpu: 65%      # CPU each pod should run at during the peak (default 65%)
  current_pods: 4      # pods running today, to compare with (optional)
  spare_pods: 1        # so one pod can fail or redeploy (default 1)
```

The result then shows:

```text
usable rate per pod = sustained rate per pod × min(1, target CPU ÷ CPU at that rate)
pods                = ⌈peak × (1 + growth) ÷ usable rate per pod⌉ + spare pods
```

- **Why a target CPU below 100%.** A pod at full CPU has no room for a spike, a garbage collection or a neighbour restarting. 60–70% is a common target, and it matches what a Kubernetes HorizontalPodAutoscaler usually aims for.
- **When the CPU was saturated** (90% or more of the pod) at the sustained step, CPU no longer grows with the rate, so Stormix scales the highest step that held below 90% instead.
- **When the CPU stayed below the target** and latency or errors broke first, the bottleneck is elsewhere (a database, a pool, a lock). The usable rate is then the sustained rate itself: more pods only help if that bottleneck scales with them.
- **Without a monitor** the sustained step is taken as saturated, so the usable rate is that rate × the target CPU.

In the app, the result's **Pods needed** box starts from the `capacity` section, and you can change the peak, growth and pod counts there to try other numbers without running the test again.

## Pools: Little's law

The result also shows how many requests each pod has in flight at its usable rate: rate × average latency (Little's law). 150 req/s at 40 ms is 6 requests in flight. Size the pod's thread pool and its database connection pool above that, with headroom for latency spikes.

## Example

The actuator-demo app on one core (`docker run --cpus=1`), with `p95 < 250ms`, `errors < 1%` and `cpu < 85%`:

```text
  limit steps (each checked on its own)
          rate      req/s        avg        p95   errors   api cpu
    ✓     10/s       19.9    10.9 ms    21.0 ms    0.00%       22%
    ✓     20/s       39.9    10.8 ms    20.9 ms    0.00%       42%
    ✓     30/s       60.1    10.7 ms    20.8 ms    0.00%       62%
    ✓     40/s       79.9    10.7 ms    20.8 ms    0.00%       83%  ← sustained
    ✗     50/s      100.0    11.3 ms    21.0 ms    0.00%      100%  broke cpu < 85% (100%)

  limit       79.9 req/s sustained (40/s iterations); the next step broke a threshold

  pods needed 14
    one pod held   79.9 req/s at 83% CPU of 1 core(s)
    usable per pod 62.5 req/s (scaled to the 65% CPU target)
    design rate    780.0 req/s = peak 600 × (1 + 30% growth)
    pods           ⌈780.0 ÷ 62.5⌉ = 13 + 1 spare = 14
    running now    4: add 10
```

The estimate assumes pods scale out evenly. Shared bottlenecks such as one database do not, so confirm the final count with a `constant_rate` test at the design rate against that many pods.
