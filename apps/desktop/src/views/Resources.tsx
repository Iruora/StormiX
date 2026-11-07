import { Cpu, HeartPulse, TriangleAlert } from "lucide-react";

import type { LoadProgress, LoadReport } from "../bindings";

/** One monitored API's readings, from a finished report or gathered live. */
export type ResourceView = {
  name: string;
  url?: string;
  samples: { t: number | null; cpuPercent?: number | null; memoryBytes?: number | null; heapBytes?: number | null; gcMsPerS?: number | null }[];
  error?: string | null;
  failedReads?: number;
  cpuCores?: number | null;
  memoryKind?: string | null;
  heapMaxBytes?: number | null;
  healthUrl?: string | null;
  /** The first health reading, then each change. */
  health?: HealthChange[];
  healthError?: string | null;
};

export type HealthChange = { t: number | null; status: string; detail?: string | null };

/** `UP` and `UNKNOWN` are not counted against the API. */
const unhealthy = (h: HealthChange) => h.status !== "UP" && h.status !== "UNKNOWN";

/** Each stretch the API was not healthy; `to` is null when it lasted until the end. */
export function unhealthyPeriods(health: HealthChange[] = []) {
  const periods: { from: number; to: number | null; status: string; detail?: string | null }[] = [];
  health.forEach((h, i) => {
    if (!unhealthy(h) || (i > 0 && unhealthy(health[i - 1]))) return;
    const back = health.slice(i + 1).find((c) => !unhealthy(c));
    periods.push({ from: h.t ?? 0, to: back?.t ?? null, status: h.status, detail: h.detail });
  });
  return periods;
}

/** Whether any monitored API was unhealthy at some point of the test. */
export function wentUnhealthy(series: ResourceView[]): boolean {
  return series.some((s) => unhealthyPeriods(s.health).length > 0);
}

/** A finished report's series. */
export function reportResources(report: LoadReport): ResourceView[] {
  return (report.resources ?? []).map((r) => ({ ...r }));
}

/**
 * Builds series by monitor index from events in time order, using the latest
 * event's monitor list and metadata. Repeated sample timestamps are omitted;
 * health changes use the event's elapsed seconds. Returns no series without
 * monitors in the latest event.
 */
export function liveResources(points: LoadProgress[]): ResourceView[] {
  const last = points[points.length - 1];
  return (last?.resources ?? []).map((r, i) => {
    const seen = new Set<number>();
    const samples: ResourceView["samples"] = [];
    const health: HealthChange[] = [];
    for (const p of points) {
      const now = p.resources?.[i];
      if (now?.health) {
        const last = health[health.length - 1];
        if (!last || last.status !== now.health || last.detail !== now.healthDetail) {
          health.push({ t: p.elapsedS, status: now.health, detail: now.healthDetail });
        }
      }
      if (now?.t == null || seen.has(now.t)) continue;
      seen.add(now.t);
      samples.push({ t: now.t, cpuPercent: now.cpuPercent, memoryBytes: now.memoryBytes });
    }
    return { name: r.name, samples, error: r.error, cpuCores: r.cpuCores, memoryKind: r.memoryKind, health };
  });
}

/**
 * The API's own CPU and memory beside the load: figures, then both as lines
 * over the same time axis as the throughput chart. `live` shows the latest
 * reading instead of averages and peaks.
 */
export function ApiResources({ series, duration, live = false }: { series: ResourceView[]; duration: number; live?: boolean }) {
  if (series.length === 0) return null;
  return (
    <>
      {series.map((s, i) => (
        <OneApi key={i} series={s} duration={duration} live={live} />
      ))}
    </>
  );
}

/** One monitored API: health, figures and chart. */
function OneApi({ series, duration, live }: { series: ResourceView; duration: number; live: boolean }) {
  const cpu = values(series, (s) => s.cpuPercent);
  const memory = values(series, (s) => s.memoryBytes);
  const heap = values(series, (s) => s.heapBytes);
  const gc = values(series, (s) => s.gcMsPerS);
  const cores = series.cpuCores ? ` of ${Math.round(series.cpuCores)} cores` : "";
  const memoryWhat = series.memoryKind === "jvm" ? "JVM heap + non-heap" : "resident memory";
  const empty = series.samples.length === 0;
  return (
    <div className="border-t border-line px-4 py-3">
      <div className="mb-2 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <span className="flex items-center gap-1.5 text-xs font-medium text-subtle">
          <Cpu size="0.8125rem" /> API resources
        </span>
        <span className="text-[0.8125rem] font-medium">{series.name}</span>
        {series.url && (
          <span className="mono min-w-0 truncate text-xs text-muted" title={series.url}>
            {series.url}
          </span>
        )}
      </div>
      <Health series={series} live={live} />
      {series.error && (
        <div className="mb-2 flex items-start gap-1.5 text-xs text-warn">
          <TriangleAlert size="0.8125rem" className="mt-px shrink-0" />
          <span>
            {empty
              ? `Could not read it: ${series.error}`
              : `${series.failedReads ? `${series.failedReads} reads failed, the last with` : "The latest read failed:"} ${series.error}`}
          </span>
        </div>
      )}
      {empty ? (
        !series.error && <div className="text-xs text-muted">Waiting for the first reading…</div>
      ) : (
        <>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(6rem,1fr))] gap-3">
            {live ? (
              <>
                <Figure label={`CPU now (100% = 1 core${cores})`} value={pct(cpu.at(-1))} />
                <Figure label={`memory now, ${memoryWhat}`} value={bytes(memory.at(-1))} />
                <Figure label="CPU peak" value={pct(max(cpu))} />
                <Figure label="memory peak" value={bytes(max(memory))} />
              </>
            ) : (
              <>
                <Figure label="CPU avg" value={pct(avg(cpu))} />
                <Figure label="CPU peak" value={pct(max(cpu))} />
                <Figure label="memory before" value={bytes(memory[0])} />
                <Figure label="memory peak" value={bytes(max(memory))} />
                <Figure label="memory at the end" value={bytes(memory.at(-1))} />
                {heap.length > 0 && (
                  <Figure
                    label="heap peak"
                    value={`${bytes(max(heap))}${series.heapMaxBytes ? ` / ${bytes(series.heapMaxBytes)}` : ""}`}
                  />
                )}
                {gc.length > 0 && <Figure label="GC pauses" value={`${avg(gc)!.toFixed(1)} ms/s`} />}
              </>
            )}
          </div>
          {!live && (
            <div className="mt-1 text-[0.6875rem] text-subtle">
              CPU: 100% is one full core{cores}. Memory: {memoryWhat}. The first reading is taken just before the load starts.
            </div>
          )}
          <Chart series={series} duration={duration} />
        </>
      )}
    </div>
  );
}

/** Healthy throughout, or each time it was not; live, the status now. */
function Health({ series, live }: { series: ResourceView; live: boolean }) {
  const health = series.health ?? [];
  if (series.healthError) {
    return <div className="mb-2 text-xs text-muted">Health not checked: {series.healthError}</div>;
  }
  if (health.length === 0) return null;
  const now = health[health.length - 1];
  const periods = unhealthyPeriods(health);
  if (live && unhealthy(now)) {
    return (
      <div role="status" className="mb-2 flex items-start gap-1.5 rounded-md bg-bad-soft px-2 py-1 text-xs text-bad">
        <HeartPulse size="0.8125rem" className="mt-px shrink-0" />
        <span>
          <b>{now.status}</b> since {periods.at(-1)?.from ?? 0}s{now.detail ? ` · ${now.detail}` : ""}
        </span>
      </div>
    );
  }
  if (periods.length === 0) {
    return (
      <div className="mb-2 flex items-center gap-1.5 text-xs text-ok">
        <HeartPulse size="0.8125rem" /> {live ? "Healthy" : "Healthy throughout"}
      </div>
    );
  }
  return (
    <ul className="mb-2 space-y-0.5 rounded-md bg-bad-soft px-2 py-1 text-xs text-bad">
      {periods.map((p, i) => (
        <li key={i} className="flex items-start gap-1.5">
          <HeartPulse size="0.8125rem" className="mt-px shrink-0" />
          <span>
            <b>{p.status}</b> at {p.from.toFixed(1)} s{p.detail ? ` (${p.detail})` : ""}
            {p.to == null
              ? ", still at the end"
              : `, healthy again at ${p.to.toFixed(1)} s, after ${(p.to - p.from).toFixed(1)} s`}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** CPU and memory as two lines, each scaled to its own peak, with unhealthy stretches shaded. */
function Chart({ series, duration }: { series: ResourceView; duration: number }) {
  const points = series.samples.filter((s) => s.t != null) as (ResourceView["samples"][number] & { t: number })[];
  if (points.length < 2) return null;
  const width = 600;
  const height = 80;
  const end = Math.max(1, duration, points[points.length - 1].t);
  const line = (pick: (s: (typeof points)[number]) => number | null | undefined) => {
    const got = points.flatMap((s) => {
      const v = pick(s);
      return v == null ? [] : [[s.t, v] as const];
    });
    const top = Math.max(1e-9, ...got.map(([, v]) => v));
    return {
      top,
      points: got.map(([t, v]) => `${(t / end) * width},${height - (v / top) * (height - 6)}`).join(" "),
    };
  };
  const cpu = line((s) => s.cpuPercent);
  const memory = line((s) => s.memoryBytes);
  const bands = unhealthyPeriods(series.health);
  return (
    <div className="mt-2">
      <div className="mb-1 flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-subtle">
        <span className="flex items-center gap-1">
          <span className="inline-block h-0.5 w-3 bg-accent" /> CPU (max {pct(cpu.top)})
        </span>
        <span className="flex items-center gap-1">
          <span className="inline-block h-0.5 w-3 bg-ok" /> memory (max {bytes(memory.top)})
        </span>
        {bands.length > 0 && (
          <span className="flex items-center gap-1">
            <span className="inline-block size-2 rounded-sm bg-bad/30" /> unhealthy
          </span>
        )}
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="h-20 w-full"
        preserveAspectRatio="none"
        role="img"
        aria-label={`${series.name}: CPU and memory over time`}
      >
        {bands.map((b, i) => {
          const x = (b.from / end) * width;
          return <rect key={i} x={x} y={0} width={Math.max(2, ((b.to ?? points[points.length - 1].t) / end) * width - x)} height={height} className="fill-bad/15" />;
        })}
        <polyline points={cpu.points} fill="none" className="stroke-accent" strokeWidth={2} vectorEffect="non-scaling-stroke" />
        <polyline points={memory.points} fill="none" className="stroke-ok" strokeWidth={2} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="mono mt-0.5 flex justify-between text-[0.6875rem] text-subtle">
        <span>0s</span>
        <span>{Math.round(end / 2)}s</span>
        <span>{Math.round(end)}s</span>
      </div>
    </div>
  );
}

/** One big number with its label. */
function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="break-words text-base font-semibold tabular-nums">{value}</div>
      <div className="text-xs text-subtle">{label}</div>
    </div>
  );
}

/** The readings of one kind, leaving out reads that lacked it. */
function values(series: ResourceView, pick: (s: ResourceView["samples"][number]) => number | null | undefined): number[] {
  return series.samples.flatMap((s) => {
    const v = pick(s);
    return v == null ? [] : [v];
  });
}

const avg = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : undefined);
const max = (v: number[]) => (v.length ? Math.max(...v) : undefined);

/** A CPU figure, or a dash when there is none. */
function pct(v: number | undefined): string {
  return v == null ? "–" : `${Math.round(v)}%`;
}

/**
 * Formats bytes using powers of 1024, labeled MB or GB from 1024 MB up.
 * MB is rounded to a whole number, GB has two decimals, and missing values show a dash.
 */
export function bytes(v: number | null | undefined): string {
  if (v == null) return "–";
  const mb = v / 1024 / 1024;
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${Math.round(mb)} MB`;
}
