# Load tests in the app

Click **Load test** in the header. A setup dialog opens first, then the test runs with a live chart, then the results stay in the panel and in the [Runs](/app/runs) page. [Load testing](/guide/load-testing) explains the models and thresholds behind it.

## The setup dialog

<Shot name="load-setup" alt="The load test setup dialog" />

- **Endpoints**: every step of the flow, all selected, with a checkbox each and one to select or clear them all. Load test only the endpoints you care about; a warning shows when a selected step uses a variable that only a left-out step sets. Steps turned off in the flow are not listed. Your choice is kept in the [workspace](/app/workspaces) for next time.
- **Cache**: turn a step's [response cache](/features/cache) on here to make it a prerequisite, such as a login run once before the load.
- **Load profile**: constant rate, ramping rate with stages, or virtual users.
- **Test data**: lists of values, such as user IDs, typed in or imported from a CSV file, that each iteration or virtual user picks from. See [Test data](/features/test-data).
- **Thresholds**: such as `p95 < 300ms` or `errors < 1%`.
- **Trace requests**: send a `traceparent` header for [APM tools](/features/tracing).
- **API CPU and memory**: metrics endpoints of the API, such as `/actuator/prometheus`, to read its [CPU, memory and health](/features/api-resources) from during the test.

The load profile, test data and thresholds are saved into the test file. A file without a load profile starts from 10 iterations a second for 30 seconds. **Run** starts the test with the selected steps; **Cancel** closes the dialog.

## While it runs

The panel says the test is starting (setup steps and warm-up) until its first second is in, then draws a chart of requests per second, failures and p95 latency for each second. With a metrics endpoint set up, the API's current CPU and memory and their chart show below it, and a red banner as soon as its health check says it is down.

<Shot name="load-running" alt="A load test running, with its live chart" />

### Stopping a test

Started the wrong test, or pointed it at the wrong server? **Stop** (in the header and on the live panel) ends it early. No new iterations start, the requests already in flight get a few seconds to finish, and the panel then shows what was measured so far, marked **Stopped before the end**. A stopped test never counts as passed, even when its thresholds held, and it is kept in the run history like any other.

## The results

At the end the panel shows:

- requests, iterations, failures, checks and dropped iterations;
- latency: min, average, p50, p90, p95, p99 and max;
- each **threshold** with its measured value, passed or failed;
- the per-second chart;
- a **per-step table**;
- the **most common failures**, each with a **Details** button that opens the first failing request and its response;
- the **slowest traced requests**, with tracing on;
- the **API's CPU and memory**: average and peak CPU, memory before, at its peak and at the end, JVM heap and GC pauses, a chart, and whether it stayed healthy (with **API went unhealthy** in the header when it did not), for each metrics endpoint set up.

**Replay** runs the same steps again with the same load profile and thresholds, without going through the setup dialog. It uses the environment selected now. Editing the file clears the result, and with it Replay; use **Load test** to start from the setup again.

### Sorting and filtering steps

Click a column header of the per-step table to sort by it, again to flip the order, a third time to go back to flow order. Filter by step name, by **Failing only**, or by a minimum p95, to find the slow or broken endpoint in a large collection.

<Shot name="load-sorted" alt="The per-step table sorted by p95" />

## Next

- Compare this test with the previous one on the [Runs](/app/runs) page to see whether latency moved.
- Export it as an [HTML report](/app/runs#exporting) to share.
- Run the same test in [CI](/guide/ci) with `stormix load`.
