import { CircleCheck, CircleX, LoaderCircle } from "lucide-react";

import type { LoadProgress, LoadReport, Second, ThresholdResult } from "../bindings";

/** Live figures while a load test runs: progress, the current second and a throughput chart. */
export function LoadLive({ points }: { points: LoadProgress[] }) {
  const last = points[points.length - 1];
  const total = last?.totalS ?? 0;
  const elapsed = Math.min(last?.elapsedS ?? 0, total);
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-2 border-b border-line px-4 py-3">
        <LoaderCircle size={16} className="animate-spin text-accent" />
        <span className="font-semibold">Load test running</span>
        <span className="ml-auto text-xs text-muted">
          {elapsed}s of {total}s
        </span>
      </div>
      <div className="h-1 bg-surface-2">
        <div className="h-1 bg-accent transition-all" style={{ width: total ? `${(elapsed / total) * 100}%` : "0%" }} />
      </div>
      <div className="grid grid-cols-4 gap-3 px-4 py-3">
        <Stat label="req/s" value={last ? String(last.requests) : "–"} />
        <Stat label="p95" value={last ? formatMs(last.p95Ms ?? 0) : "–"} />
        <Stat label="failed/s" value={last ? String(last.failed) : "–"} tone={last && last.failed > 0 ? "bad" : undefined} />
        <Stat label="in flight" value={last ? String(last.active) : "–"} />
      </div>
      <Timeline seconds={points.map((p) => ({ t: p.elapsedS, requests: p.requests, failed: p.failed, p95Ms: p.p95Ms }))} />
    </section>
  );
}

/** The outcome of a load test: headline figures, thresholds, a timeline and each step. */
export function LoadResult({ report }: { report: LoadReport }) {
  const s = report.summary;
  const passed = report.thresholds.every((t) => t.passed);
  const errorRate = s.requests ? (s.failedRequests / s.requests) * 100 : 0;
  const l = s.latency;
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-2 border-b border-line px-4 py-3">
        {passed ? <CircleCheck size={16} className="text-ok" /> : <CircleX size={16} className="text-bad" />}
        <span className="font-semibold">
          {report.thresholds.length === 0 ? "Load test finished" : passed ? "Thresholds held" : "Thresholds failed"}
        </span>
        <span className="ml-auto text-xs text-muted">
          {s.iterations} iterations · {formatMs(s.durationMs ?? 0)}
        </span>
      </div>

      <div className="grid grid-cols-4 gap-3 px-4 py-3">
        <Stat label="requests" value={String(s.requests)} />
        <Stat label="req/s" value={(s.rps ?? 0).toFixed(1)} />
        <Stat label="p95" value={formatMs(l.p95 ?? 0)} />
        <Stat label="failed" value={`${errorRate.toFixed(2)}%`} tone={s.failedRequests > 0 ? "bad" : "ok"} />
      </div>

      <div className="mono flex flex-wrap gap-x-4 gap-y-1 border-t border-line px-4 py-2 text-xs text-muted">
        {(["min", "avg", "p50", "p90", "p95", "p99", "max"] as const).map((k) => (
          <span key={k}>
            {k} <span className="text-ink">{formatMs(l[k] ?? 0)}</span>
          </span>
        ))}
      </div>

      {report.thresholds.length > 0 && (
        <ul className="space-y-1 border-t border-line px-4 py-3">
          {report.thresholds.map((t) => (
            <Threshold key={t.threshold} result={t} />
          ))}
        </ul>
      )}

      <Timeline seconds={s.timeline} />

      {s.steps.length > 0 && (
        <table className="w-full border-t border-line text-[13px]">
          <thead className="text-xs text-subtle">
            <tr>
              <th className="w-1/2 px-4 py-1.5 text-left font-medium">Step</th>
              <th className="px-2 py-1.5 text-right font-medium">Requests</th>
              <th className="px-2 py-1.5 text-right font-medium">Failed</th>
              <th className="px-2 py-1.5 text-right font-medium">p50</th>
              <th className="px-4 py-1.5 text-right font-medium">p95</th>
            </tr>
          </thead>
          <tbody className="mono whitespace-nowrap text-xs">
            {s.steps.map((step) => (
              <tr key={step.name} className="border-t border-line">
                <td className="max-w-0 truncate px-4 py-1.5 font-sans text-[13px]" title={step.name}>
                  {step.name}
                </td>
                <td className="px-2 py-1.5 text-right">{step.requests}</td>
                <td className={`px-2 py-1.5 text-right ${step.failed > 0 ? "text-bad" : ""}`}>{step.failed}</td>
                <td className="px-2 py-1.5 text-right">{formatMs(step.latency.p50 ?? 0)}</td>
                <td className="px-4 py-1.5 text-right">{formatMs(step.latency.p95 ?? 0)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {s.failures.length > 0 && (
        <div className="border-t border-line px-4 py-3">
          <div className="mb-1 text-xs font-medium text-subtle">Most common failures</div>
          <ul className="space-y-0.5 text-[13px]">
            {s.failures.map((f) => (
              <li key={f.reason} className="flex gap-2">
                <span className="mono w-12 shrink-0 text-right text-xs text-bad">{f.count}×</span>
                <span className="min-w-0 break-words">{f.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {(s.droppedIterations > 0 || s.interruptedIterations > 0) && (
        <div className="border-t border-line px-4 py-2 text-xs text-warn">
          {s.droppedIterations > 0 && `${s.droppedIterations} iterations dropped: too many were already in flight. `}
          {s.interruptedIterations > 0 && `${s.interruptedIterations} iterations were cut off at the end.`}
        </div>
      )}
    </section>
  );
}

function Threshold({ result }: { result: ThresholdResult }) {
  const metric = result.threshold.split(/[\s<>]/)[0];
  const actual = result.actual ?? 0;
  const measured =
    metric === "errors" || metric === "checks"
      ? `${actual.toFixed(2)}%`
      : metric === "rps"
        ? `${actual.toFixed(1)}/s`
        : formatMs(actual);
  return (
    <li className="flex items-center gap-2 text-[13px]">
      {result.passed ? (
        <CircleCheck size={14} className="shrink-0 text-ok" />
      ) : (
        <CircleX size={14} className="shrink-0 text-bad" />
      )}
      <span className="mono text-xs">{result.threshold}</span>
      <span className={`ml-auto mono text-xs ${result.passed ? "text-muted" : "text-bad"}`}>{measured}</span>
    </li>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "ok" | "bad" }) {
  const color = tone === "bad" ? "text-bad" : tone === "ok" ? "text-ok" : "text-ink";
  return (
    <div>
      <div className={`text-lg font-semibold tabular-nums ${color}`}>{value}</div>
      <div className="text-xs text-subtle">{label}</div>
    </div>
  );
}

type Point = Pick<Second, "t" | "requests" | "failed" | "p95Ms">;

/** Requests per second as bars (failures in red), p95 latency as a line on its own scale. */
function Timeline({ seconds }: { seconds: Point[] }) {
  if (seconds.length < 2) return null;
  const width = 600;
  const height = 96;
  const maxReq = Math.max(1, ...seconds.map((s) => s.requests));
  const maxP95 = Math.max(1, ...seconds.map((s) => s.p95Ms ?? 0));
  const bar = width / seconds.length;
  const y = (v: number, max: number) => height - (v / max) * (height - 8);
  const line = seconds
    .map((s, i) => `${(i + 0.5) * bar},${y(s.p95Ms ?? 0, maxP95)}`)
    .join(" ");
  return (
    <div className="border-t border-line px-4 py-3">
      <div className="mb-1 flex gap-3 text-xs text-subtle">
        <span className="flex items-center gap-1">
          <span className="inline-block size-2 rounded-sm bg-accent/60" /> req/s (max {maxReq})
        </span>
        <span className="flex items-center gap-1">
          <span className="inline-block h-0.5 w-3 bg-warn" /> p95 (max {formatMs(maxP95)})
        </span>
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} className="h-24 w-full" preserveAspectRatio="none" role="img" aria-label="Throughput and p95 latency over time">
        {seconds.map((s, i) => {
          const ok = s.requests - s.failed;
          return (
            <g key={i}>
              <rect x={i * bar + 1} width={Math.max(bar - 2, 1)} y={y(s.requests, maxReq)} height={height - y(s.requests, maxReq)} className="fill-accent/60" />
              {s.failed > 0 && (
                <rect x={i * bar + 1} width={Math.max(bar - 2, 1)} y={y(s.requests, maxReq)} height={y(ok, maxReq) - y(s.requests, maxReq)} className="fill-bad" />
              )}
            </g>
          );
        })}
        <polyline points={line} fill="none" className="stroke-warn" strokeWidth={2} vectorEffect="non-scaling-stroke" />
      </svg>
    </div>
  );
}

function formatMs(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  if (ms >= 100) return `${Math.round(ms)} ms`;
  return `${ms.toFixed(1)} ms`;
}
