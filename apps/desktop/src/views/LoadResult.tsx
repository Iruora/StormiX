import { ArrowDown, ArrowUp, ChevronsUpDown, CircleCheck, CircleX, LoaderCircle, Search } from "lucide-react";
import { useMemo, useState } from "react";

import type { LoadProgress, LoadReport, Second, StepResult, StepSummary, ThresholdResult } from "../bindings";
import { DetailsButton, stepPassed } from "./RunResult";
import { StepDetails } from "./StepDetails";

/** Live figures while a load test runs: progress, the current second and a throughput chart. */
export function LoadLive({ points }: { points: LoadProgress[] }) {
  const last = points[points.length - 1];
  const total = last?.totalS ?? 0;
  const elapsed = Math.min(last?.elapsedS ?? 0, total);
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-4 py-3">
        <LoaderCircle size="1rem" className="animate-spin text-accent" />
        <span className="font-semibold">Load test running</span>
        <span className="ml-auto text-xs text-muted">
          {elapsed}s of {total}s
        </span>
      </div>
      <div className="h-1 bg-surface-2">
        <div className="h-1 bg-accent transition-all" style={{ width: total ? `${(elapsed / total) * 100}%` : "0%" }} />
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(5rem,1fr))] gap-3 px-4 py-3">
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
  const samples = report.samples ?? [];
  const [open, setOpen] = useState<StepResult | null>(null);
  // Reasons are `step: check`, `step: status N`, or a connection error.
  const sampleFor = (reason: string) =>
    samples.find((x) => reason.startsWith(`${x.name}: `) || (x.error != null && reason === x.error));
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-4 py-3">
        {passed ? <CircleCheck size="1rem" className="text-ok" /> : <CircleX size="1rem" className="text-bad" />}
        <span className="font-semibold">
          {report.thresholds.length === 0 ? "Load test finished" : passed ? "Thresholds held" : "Thresholds failed"}
        </span>
        <span className="ml-auto text-xs text-muted">
          {s.iterations} iterations · {formatMs(s.durationMs ?? 0)}
        </span>
      </div>

      {(report.setup ?? []).length > 0 && (
        <div className="border-b border-line px-4 py-2 text-xs">
          <span className="text-subtle">Set up once before the load, not measured:</span>
          {(report.setup ?? []).map((st, i) => (
            <span key={i} className={`ml-2 inline-flex items-center gap-1 ${stepPassed(st) ? "text-muted" : "text-bad"}`}>
              {stepPassed(st) ? <CircleCheck size="0.75rem" className="text-ok" /> : <CircleX size="0.75rem" />}
              {st.name}
              <span className="mono">
                {st.error ?? `${st.status ?? "—"} · ${st.cached ? "cached" : formatMs(st.durationMs ?? 0)}`}
              </span>
              {!stepPassed(st) && <DetailsButton failed onClick={() => setOpen(st)} />}
            </span>
          ))}
        </div>
      )}

      <div className="grid grid-cols-[repeat(auto-fit,minmax(5rem,1fr))] gap-3 px-4 py-3">
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

      {s.steps.length > 0 && <StepTable steps={s.steps} />}

      {s.failures.length > 0 && (
        <div className="border-t border-line px-4 py-3">
          <div className="mb-1 text-xs font-medium text-subtle">Most common failures</div>
          <ul className="space-y-0.5 text-[0.8125rem]">
            {s.failures.map((f) => {
              const sample = sampleFor(f.reason);
              return (
                <li key={f.reason} className="flex items-start gap-2">
                  <span className="mono w-12 shrink-0 pt-px text-right text-xs text-bad">{f.count}×</span>
                  <span className="min-w-0 flex-1 break-words">{f.reason}</span>
                  {sample && <DetailsButton failed onClick={() => setOpen(sample)} />}
                </li>
              );
            })}
          </ul>
        </div>
      )}
      {!!s.cachedResponses && (
        <div className="border-t border-line px-4 py-2 text-xs text-muted">
          {s.cachedResponses} responses came from the cache, for steps marked cached. They were not sent, so they are
          not in the request count or the latency figures.
        </div>
      )}
      {(s.droppedIterations > 0 || s.interruptedIterations > 0) && (
        <div className="border-t border-line px-4 py-2 text-xs text-warn">
          {s.droppedIterations > 0 && `${s.droppedIterations} iterations dropped: too many were already in flight. `}
          {s.interruptedIterations > 0 && `${s.interruptedIterations} iterations were cut off at the end.`}
        </div>
      )}
      {open && <StepDetails step={open} onClose={() => setOpen(null)} />}
    </section>
  );
}

type Column = "name" | "requests" | "failed" | "p50" | "p95";

const COLUMNS: { key: Column; label: string; value: (s: StepSummary) => number | string }[] = [
  { key: "name", label: "Step", value: (s) => s.name },
  { key: "requests", label: "Requests", value: (s) => s.requests },
  { key: "failed", label: "Failed", value: (s) => s.failed },
  { key: "p50", label: "p50", value: (s) => s.latency.p50 ?? 0 },
  { key: "p95", label: "p95", value: (s) => s.latency.p95 ?? 0 },
];

/**
 * Each step's figures. A header click sorts by that column (again to flip
 * the order, a third time to go back to flow order); the bar above filters
 * by name, failures and slowness.
 */
function StepTable({ steps }: { steps: StepSummary[] }) {
  const [sort, setSort] = useState<{ key: Column; desc: boolean } | null>(null);
  const [query, setQuery] = useState("");
  const [failingOnly, setFailingOnly] = useState(false);
  const [slowerThan, setSlowerThan] = useState("");

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    const minP95 = Number(slowerThan) || 0;
    const kept = steps.filter(
      (s) =>
        (!q || s.name.toLowerCase().includes(q)) && (!failingOnly || s.failed > 0) && (s.latency.p95 ?? 0) >= minP95,
    );
    if (!sort) return kept;
    const value = COLUMNS.find((c) => c.key === sort.key)!.value;
    return [...kept].sort((a, b) => {
      const x = value(a);
      const y = value(b);
      const order = typeof x === "string" ? x.localeCompare(y as string) : x - (y as number);
      return sort.desc ? -order : order;
    });
  }, [steps, sort, query, failingOnly, slowerThan]);

  const clickHeader = (key: Column) =>
    setSort((current) => {
      // Numbers start largest first, names A to Z.
      const firstDesc = key !== "name";
      if (current?.key !== key) return { key, desc: firstDesc };
      if (current.desc === firstDesc) return { key, desc: !firstDesc };
      return null;
    });
  const filtered = query.trim() !== "" || failingOnly || Number(slowerThan) > 0;

  return (
    <div className="border-t border-line">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-4 py-2 text-xs">
        <label className="flex min-w-36 flex-1 items-center gap-1.5 rounded-md border border-line px-2 py-0.5 focus-within:border-accent">
          <Search size="0.75rem" className="text-subtle" />
          <input
            aria-label="Filter steps by name"
            placeholder="Filter steps"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full bg-transparent outline-none placeholder:text-subtle"
          />
        </label>
        <label className="flex items-center gap-1.5 text-muted">
          <input
            type="checkbox"
            checked={failingOnly}
            onChange={(e) => setFailingOnly(e.target.checked)}
            className="size-3.5 accent-accent"
          />
          Failing only
        </label>
        <label className="flex items-center gap-1.5 text-muted">
          p95 ≥
          <input
            aria-label="Only steps with a p95 of at least, in ms"
            inputMode="numeric"
            placeholder="0"
            value={slowerThan}
            onChange={(e) => /^\d*\.?\d*$/.test(e.target.value) && setSlowerThan(e.target.value)}
            className="mono w-14 rounded-md border border-line bg-surface px-1.5 py-0.5 text-right outline-none focus:border-accent"
          />
          ms
        </label>
        {filtered && (
          <span className="ml-auto text-subtle">
            {rows.length} of {steps.length}
          </span>
        )}
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-[0.8125rem]">
          <thead className="whitespace-nowrap text-xs text-subtle">
            <tr>
              {COLUMNS.map((c, i) => {
                const active = sort?.key === c.key;
                const Icon = active ? (sort.desc ? ArrowDown : ArrowUp) : ChevronsUpDown;
                return (
                  <th
                    key={c.key}
                    aria-sort={active ? (sort.desc ? "descending" : "ascending") : "none"}
                    className={`py-1.5 font-medium ${c.key === "name" ? "w-2/5 min-w-28 px-4 text-left" : i === COLUMNS.length - 1 ? "px-4 text-right" : "px-2 text-right"}`}
                  >
                    <button
                      type="button"
                      onClick={() => clickHeader(c.key)}
                      className={`inline-flex items-center gap-0.5 hover:text-ink ${active ? "text-ink" : ""} ${c.key === "name" ? "" : "flex-row-reverse"}`}
                    >
                      {c.label}
                      <Icon size="0.6875rem" className={active ? "" : "opacity-40"} />
                    </button>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody className="mono whitespace-nowrap text-xs">
            {rows.map((step, i) => (
              <tr key={`${step.name}-${i}`} className="border-t border-line">
                <td className="max-w-0 truncate px-4 py-1.5 font-sans text-[0.8125rem]" title={step.name}>
                  {step.name}
                </td>
                <td
                  className="px-2 py-1.5 text-right"
                  title={step.cached ? `Plus ${step.cached} responses reused from the cache, not sent` : undefined}
                >
                  {step.requests}
                  {!!step.cached && <span className="ml-1 text-accent">+{step.cached} cached</span>}
                </td>
                <td
                  className={`px-2 py-1.5 text-right ${step.failed > 0 ? "text-bad" : ""}`}
                  title={step.requests ? `${((step.failed / step.requests) * 100).toFixed(2)}% of requests` : undefined}
                >
                  {step.failed}
                </td>
                <td className="px-2 py-1.5 text-right">{step.requests ? formatMs(step.latency.p50 ?? 0) : "–"}</td>
                <td className="px-4 py-1.5 text-right">{step.requests ? formatMs(step.latency.p95 ?? 0) : "–"}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr className="border-t border-line">
                <td colSpan={COLUMNS.length} className="px-4 py-3 text-center font-sans text-[0.8125rem] text-subtle">
                  No step matches these filters.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
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
    <li className="flex items-center gap-2 text-[0.8125rem]">
      {result.passed ? (
        <CircleCheck size="0.875rem" className="shrink-0 text-ok" />
      ) : (
        <CircleX size="0.875rem" className="shrink-0 text-bad" />
      )}
      <span className="mono min-w-0 break-words text-xs">{result.threshold}</span>
      <span className={`mono ml-auto shrink-0 text-xs ${result.passed ? "text-muted" : "text-bad"}`}>{measured}</span>
    </li>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "ok" | "bad" }) {
  const color = tone === "bad" ? "text-bad" : tone === "ok" ? "text-ok" : "text-ink";
  return (
    <div>
      <div className={`break-words text-lg font-semibold tabular-nums ${color}`}>{value}</div>
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
      <div className="mb-1 flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-subtle">
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
        <line x1={0} x2={width} y1={y(maxReq / 2, maxReq)} y2={y(maxReq / 2, maxReq)} className="stroke-line" strokeDasharray="4 4" vectorEffect="non-scaling-stroke" />
        <polyline points={line} fill="none" className="stroke-warn" strokeWidth={2} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="mono mt-0.5 flex justify-between text-[0.6875rem] text-subtle">
        <span>{seconds[0].t}s</span>
        <span>{seconds[Math.floor(seconds.length / 2)].t}s</span>
        <span>{seconds[seconds.length - 1].t}s</span>
      </div>
    </div>
  );
}

function formatMs(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  if (ms >= 100) return `${Math.round(ms)} ms`;
  return `${ms.toFixed(1)} ms`;
}
