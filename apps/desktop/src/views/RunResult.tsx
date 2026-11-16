import { CircleCheck, CircleMinus, CircleX, PanelRightOpen } from "lucide-react";
import { useState } from "react";

import type { FlowResult, RunProgress, StepResult } from "../bindings";
import { methodText } from "../lib/methods";
import { ProgressBar, Spinner } from "./Busy";
import { StepDetails, type Diagnose } from "./StepDetails";

/** While a single run goes: which step it is on, out of how many. */
export function RunLive({ name, progress }: { name: string; progress: RunProgress | null }) {
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface" aria-busy>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-4 py-3">
        <Spinner size="1rem" className="text-accent" />
        <span className="min-w-0 truncate font-semibold">Running {name}</span>
        <span className="ml-auto text-xs tabular-nums text-muted">
          {progress ? `Step ${progress.index + 1} of ${progress.total}` : "Starting"}
        </span>
      </div>
      <ProgressBar className="rounded-none" value={progress ? progress.index / Math.max(1, progress.total) : null} />
      <div className="truncate px-4 py-2.5 text-xs text-muted">
        {progress ? (
          <>
            Waiting for <span className="text-ink">{progress.name}</span>
          </>
        ) : (
          "Preparing the requests"
        )}
      </div>
    </section>
  );
}

/** The outcome of one functional run: a summary line, then each step with what failed. */
export function RunResult({ result, diagnose }: { result: FlowResult; diagnose?: Diagnose }) {
  const ran = result.steps.filter((s) => !s.skipped);
  const failed = ran.filter((s) => !stepPassed(s)).length;
  const checks = ran.flatMap((s) => s.checks);
  const passedChecks = checks.filter((c) => c.passed).length;
  const skipped = result.steps.length - ran.length;
  const ok = failed === 0 && skipped === 0;
  const [open, setOpen] = useState<StepResult | null>(null);
  // With parallel branches, a bar per step shows when it ran.
  const timeline = overlaps(result) ? Math.max(result.durationMs ?? 0, 1) : null;

  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-4 py-3">
        {ok ? (
          <CircleCheck size="1rem" className="text-ok" />
        ) : (
          <CircleX size="1rem" className="text-bad" />
        )}
        <span className="font-semibold">{ok ? "Run passed" : "Run failed"}</span>
        {skipped > 0 && (
          <span className="text-xs text-muted">
            · {skipped} skipped after a failed step
          </span>
        )}
        <span className="ml-auto text-xs text-muted">
          {passedChecks}/{checks.length} checks · {formatMs(result.durationMs ?? 0)}
        </span>
      </div>
      <ol className="divide-y divide-line">
        {result.steps.map((step, i) => (
          <Step key={i} step={step} timeline={timeline} onDetails={() => setOpen(step)} />
        ))}
      </ol>
      {open && <StepDetails step={open} onClose={() => setOpen(null)} diagnose={diagnose} />}
    </section>
  );
}

/** Whether a step started before another had finished: the flow ran branches in parallel. */
export function overlaps(result: FlowResult): boolean {
  const spans = result.steps
    .filter((s) => s.startedMs != null)
    .map((s) => [s.startedMs!, s.startedMs! + (s.durationMs ?? 0)] as const)
    .sort((a, b) => a[0] - b[0]);
  return spans.some((s, i) => i > 0 && s[0] < spans[i - 1][1] - 0.5);
}

/** When a step ran within the run, as a bar on the run's time line. */
function TimeBar({ step, total }: { step: StepResult; total: number }) {
  const start = step.startedMs ?? 0;
  const left = (start / total) * 100;
  const width = Math.max(1.5, ((step.durationMs ?? 0) / total) * 100);
  return (
    <span
      className="relative hidden h-1.5 w-24 shrink-0 rounded-full bg-surface-2 sm:block"
      title={`Started at +${Math.round(start)} ms, took ${Math.round(step.durationMs ?? 0)} ms`}
    >
      <span
        className={`absolute inset-y-0 rounded-full ${stepPassed(step) ? "bg-accent" : "bg-bad"}`}
        style={{ left: `${Math.min(left, 98)}%`, width: `${Math.min(width, 100 - Math.min(left, 98))}%` }}
      />
    </span>
  );
}

function Step({ step, timeline, onDetails }: { step: StepResult; timeline: number | null; onDetails: () => void }) {
  const passed = stepPassed(step);
  const failedChecks = step.checks.filter((c) => !c.passed);
  const passedChecks = step.checks.filter((c) => c.passed);

  return (
    <li className="px-4 py-2.5">
      {/* The status, time and Details move under the name when the panel is narrow. */}
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        {step.skipped ? (
          <CircleMinus size="0.875rem" className="shrink-0 text-subtle" />
        ) : passed ? (
          <CircleCheck size="0.875rem" className="shrink-0 text-ok" />
        ) : (
          <CircleX size="0.875rem" className="shrink-0 text-bad" />
        )}
        <span className={`min-w-0 flex-1 basis-32 truncate ${step.skipped ? "text-muted" : ""}`}>{step.name}</span>
        <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-muted">
          {step.skipped ? (
            <span title="Not sent: a step it runs after failed, and the flow stops at a failure">
              skipped, an earlier step failed
            </span>
          ) : (
            <>
              {timeline != null && <TimeBar step={step} total={timeline} />}
              {step.status != null && <StatusPill status={step.status} />}
              {step.cached ? (
                <span
                  className="rounded bg-accent-soft px-1.5 py-px text-accent"
                  title="Reused from the cache; no request was sent. Clear it in the Flow tab."
                >
                  cached
                </span>
              ) : (
                <span className="mono">{formatMs(step.durationMs ?? 0)}</span>
              )}
              <DetailsButton failed={!passed} onClick={onDetails} />
            </>
          )}
        </span>
      </div>
      {!step.skipped && (
        <div className="mono mt-0.5 truncate pl-[1.375rem] text-xs text-subtle" title={step.url}>
          <span className={`font-semibold ${methodText(step.method)}`}>{step.method}</span> {step.url}
        </div>
      )}
      <ul className="mt-1 space-y-0.5 pl-[1.375rem] text-[0.8125rem]">
        {step.error && <li className="text-bad">{step.error}</li>}
        {failedChecks.map((c, i) => (
          <li key={`f${i}`} className="text-bad">
            {c.name}
            {c.message && <span className="text-muted">: {c.message}</span>}
          </li>
        ))}
        {step.warnings.map((w, i) => (
          <li key={`w${i}`} className="text-warn">
            {w}
          </li>
        ))}
        {passedChecks.map((c, i) => (
          <li key={`p${i}`} className="text-muted">
            {c.name}
          </li>
        ))}
      </ul>
      {step.logs.length > 0 && (
        <details className="mt-1 pl-[1.375rem]" open={!passed}>
          <summary className="cursor-pointer text-xs text-subtle">
            Console ({step.logs.length})
          </summary>
          <pre className="mono mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-surface-2 px-2 py-1 text-xs">
            {step.logs.join("\n")}
          </pre>
        </details>
      )}
    </li>
  );
}

/** Prominent on failed steps; passed ones keep a quieter one for their response. */
export function DetailsButton({ failed, onClick }: { failed: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1 rounded border px-1.5 py-px text-xs font-medium ${
        failed
          ? "border-bad/40 bg-bad-soft text-bad hover:border-bad"
          : "border-line text-muted hover:bg-surface-2 hover:text-ink"
      }`}
    >
      <PanelRightOpen size="0.75rem" />
      Details
    </button>
  );
}

function StatusPill({ status }: { status: number }) {
  const tone =
    status < 300 ? "bg-ok-soft text-ok" : status < 400 ? "bg-surface-2 text-muted" : "bg-bad-soft text-bad";
  return <span className={`mono rounded px-1.5 py-px font-semibold ${tone}`}>{status}</span>;
}

/** In a run that stopped at a failure: the step it stopped at and how many were skipped after it. */
/**
 * Only for a flow without branches: there, the failed step is the one just
 * before the first skipped one. With branches, the Flow tab works it out
 * from the graph.
 */
export function stoppedAt(run: FlowResult | null, linear = true): { name: string; skipped: number } | null {
  if (!run || !linear) return null;
  const first = run.steps.findIndex((s) => s.skipped);
  if (first <= 0) return null;
  // With branches, skipped steps need not follow one failure; the Flow tab
  // says which failure skipped which steps.
  if (!run.steps.slice(first).every((s) => s.skipped) || stepPassed(run.steps[first - 1])) return null;
  return { name: run.steps[first - 1].name, skipped: run.steps.length - first };
}

export function stepPassed(step: StepResult): boolean {
  return !step.skipped && step.error == null && step.checks.every((c) => c.passed);
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
}
