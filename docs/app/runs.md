# Runs, diffs and reports

Every **Run once** and every **Load test** is kept on the **Runs** page (the clock icon in the sidebar), newest first. The history is stored on your machine in `runs.db`, in the app's data folder.

<Shot name="runs" alt="The Runs page" single />

## Finding what changed

Open a run and click **Compare with previous**, or tick any two runs to compare them.

**For regression runs**, the comparison lists which steps broke and which were fixed, with the checks that changed and the status before and after. A run that passed before and fails now is marked as a regression in the list.

<Shot name="regression-diff" alt="Comparing two runs: two steps broke" single />

**For load tests**, it shows how throughput, p50, p95, p99, failures and checks moved, overall and per step. A move of more than 10% is marked as worse or better.

<Shot name="load-diff" alt="Comparing two load tests" single />

## Exporting

From a run, export it as:

| Format | For |
|---|---|
| **HTML report** | A single file with no scripts and no external files, which opens in any browser: attach it to a ticket or an email |
| **JSON** | The full result, including each failed request and response, for your own tooling |
| **JUnit XML** | CI test tabs: one test case per step for a run, one per threshold for a load test |

<Shot name="html-report" alt="An HTML load test report" single narrow />

The CLI writes the same reports with `--html`, `--out` and `--junit` ([CLI](/reference/cli)).

## Deleting runs

Delete a run from its page. Deleting a run does not touch the test file.
