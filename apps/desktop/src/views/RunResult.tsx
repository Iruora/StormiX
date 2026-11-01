import { CircleCheck, CircleMinus, CircleX, PanelRightOpen } from "lucide-react";
import { useState } from "react";

import type { FlowResult, StepResult } from "../bindings";
import { methodText } from "../lib/methods";
import { StepDetails } from "./StepDetails";

/** The outcome of one functional run: a summary line, then each step with what failed. */
export function RunResult({ result }: { result: FlowResult }) {
  const ran = result.steps.filter((s) => !s.skipped);
  const failed = ran.filter((s) => !stepPassed(s)).length;
  const checks = ran.flatMap((s) => s.checks);
  const passedChecks = checks.filter((c) => c.passed).length;
  const ok = failed === 0 && ran.length === result.steps.length;
  const [open, setOpen] = useState<StepResult | null>(null);

  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-2 border-b border-line px-4 py-3">
        {ok ? (
          <CircleCheck size={16} className="text-ok" />
        ) : (
          <CircleX size={16} className="text-bad" />
        )}
        <span className="font-semibold">{ok ? "Run passed" : "Run failed"}</span>
        <span className="ml-auto text-xs text-muted">
          {passedChecks}/{checks.length} checks · {formatMs(result.durationMs ?? 0)}
        </span>
      </div>
      <ol className="divide-y divide-line">
        {result.steps.map((step, i) => (
          <Step key={i} step={step} onDetails={() => setOpen(step)} />
        ))}
      </ol>
      {open && <StepDetails step={open} onClose={() => setOpen(null)} />}
    </section>
  );
}

function Step({ step, onDetails }: { step: StepResult; onDetails: () => void }) {
  const passed = stepPassed(step);
  const failedChecks = step.checks.filter((c) => !c.passed);
  const passedChecks = step.checks.filter((c) => c.passed);

  return (
    <li className="px-4 py-2.5">
      <div className="flex items-center gap-2">
        {step.skipped ? (
          <CircleMinus size={14} className="shrink-0 text-subtle" />
        ) : passed ? (
          <CircleCheck size={14} className="shrink-0 text-ok" />
        ) : (
          <CircleX size={14} className="shrink-0 text-bad" />
        )}
        <span className="min-w-0 truncate">{step.name}</span>
        <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-muted">
          {step.skipped ? (
            "skipped"
          ) : (
            <>
              {step.status != null && <StatusPill status={step.status} />}
              <span className="mono">{formatMs(step.durationMs ?? 0)}</span>
              <DetailsButton failed={!passed} onClick={onDetails} />
            </>
          )}
        </span>
      </div>
      {!step.skipped && (
        <div className="mono mt-0.5 truncate pl-[22px] text-xs text-subtle" title={step.url}>
          <span className={`font-semibold ${methodText(step.method)}`}>{step.method}</span> {step.url}
        </div>
      )}
      <ul className="mt-1 space-y-0.5 pl-[22px] text-[13px]">
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
        <details className="mt-1 pl-[22px]" open={!passed}>
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
      <PanelRightOpen size={12} />
      Details
    </button>
  );
}

function StatusPill({ status }: { status: number }) {
  const tone =
    status < 300 ? "bg-ok-soft text-ok" : status < 400 ? "bg-surface-2 text-muted" : "bg-bad-soft text-bad";
  return <span className={`mono rounded px-1.5 py-px font-semibold ${tone}`}>{status}</span>;
}

export function stepPassed(step: StepResult): boolean {
  return !step.skipped && step.error == null && step.checks.every((c) => c.passed);
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
}
