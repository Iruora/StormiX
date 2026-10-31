import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight,
  CircleCheck,
  CircleMinus,
  CirclePlus,
  CircleX,
  Download,
  Gauge,
  GitCompareArrows,
  History,
  ListChecks,
  Trash2,
} from "lucide-react";
import { useState } from "react";

import {
  commands,
  type LoadDiff,
  type Metric,
  type ReportFormat,
  type RunDiff,
  type RunSummary,
  type StepChange,
  type StepDiff,
} from "../bindings";
import { LoadResult } from "./LoadResult";
import { RunResult } from "./RunResult";

export const RUNS_KEY = ["runs"];

async function unwrap<T>(res: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>): Promise<T> {
  const r = await res;
  if (r.status === "error") throw new Error(r.error);
  return r.data;
}

/** Every run the app made: open one, export it, or compare two. */
export function RunsView() {
  const client = useQueryClient();
  const runs = useQuery({ queryKey: RUNS_KEY, queryFn: () => unwrap(commands.listRuns(500)) });
  const [selected, setSelected] = useState<number | null>(null);
  const [compare, setCompare] = useState<number[]>([]);
  const list = runs.data ?? [];
  const current = selected ?? list[0]?.id ?? null;

  const toggleCompare = (id: number) =>
    setCompare((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id].slice(-2)));
  const remove = useMutation({
    mutationFn: (id: number) => unwrap(commands.deleteRun(id)),
    onSuccess: (_, id) => {
      setCompare((c) => c.filter((x) => x !== id));
      if (selected === id) setSelected(null);
      client.invalidateQueries({ queryKey: RUNS_KEY });
    },
  });

  // The older run goes on the left, whatever order they were ticked in.
  const pair =
    compare.length === 2
      ? (compare
          .map((id) => list.find((r) => r.id === id))
          .filter((r) => r != null)
          .sort((a, b) => (a.startedAt ?? 0) - (b.startedAt ?? 0)) as [RunSummary, RunSummary])
      : null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-line bg-surface px-5 py-3">
        <h1 className="text-[15px] font-semibold">Runs</h1>
        <span className="text-xs text-muted">
          Every run is kept. Tick two to see what changed between them.
        </span>
        {compare.length > 0 && (
          <button
            type="button"
            onClick={() => setCompare([])}
            className="ml-auto rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
          >
            Clear comparison
          </button>
        )}
      </header>
      {list.length === 0 ? (
        <div className="grid flex-1 place-items-center text-center text-muted">
          <div>
            <History size={28} className="mx-auto mb-2 text-subtle" />
            {runs.isPending ? "Loading runs…" : "No runs yet. Run a test file and it shows up here."}
          </div>
        </div>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(300px,0.7fr)_minmax(0,1fr)]">
          <ul className="min-h-0 divide-y divide-line overflow-y-auto border-r border-line bg-surface">
            {list.map((run, i) => (
              <RunRow
                key={run.id}
                run={run}
                active={!pair && run.id === current}
                ticked={compare.includes(run.id)}
                onOpen={() => {
                  setSelected(run.id);
                  setCompare([]);
                }}
                onTick={() => toggleCompare(run.id)}
                previous={list.slice(i + 1).find((r) => r.name === run.name && r.kind === run.kind)}
              />
            ))}
          </ul>
          <section className="min-h-0 overflow-y-auto p-4">
            {pair ? (
              <Compare before={pair[0]} after={pair[1]} />
            ) : current != null ? (
              <RunDetail
                key={current}
                id={current}
                previous={(() => {
                  const i = list.findIndex((r) => r.id === current);
                  const run = list[i];
                  return run && list.slice(i + 1).find((r) => r.name === run.name && r.kind === run.kind);
                })()}
                onCompare={(before) => setCompare([before, current])}
                onDelete={() => remove.mutate(current)}
              />
            ) : null}
          </section>
        </div>
      )}
    </div>
  );
}

function RunRow({
  run,
  active,
  ticked,
  previous,
  onOpen,
  onTick,
}: {
  run: RunSummary;
  active: boolean;
  ticked: boolean;
  previous?: RunSummary;
  onOpen: () => void;
  onTick: () => void;
}) {
  const regressed = previous && previous.passed && !run.passed;
  return (
    <li className={`flex items-start gap-2.5 px-3 py-2.5 ${active ? "bg-accent-soft" : "hover:bg-surface-2"}`}>
      <input
        type="checkbox"
        aria-label={`Compare run ${run.id}`}
        checked={ticked}
        onChange={onTick}
        className="mt-1 accent-[var(--accent)]"
      />
      <button type="button" onClick={onOpen} className="min-w-0 flex-1 text-left">
        <div className="flex items-center gap-1.5">
          {run.passed ? (
            <CircleCheck size={14} className="shrink-0 text-ok" />
          ) : (
            <CircleX size={14} className="shrink-0 text-bad" />
          )}
          <span className="truncate font-medium">{run.name}</span>
          {regressed && (
            <span className="rounded bg-bad-soft px-1 text-[11px] font-semibold text-bad">regressed</span>
          )}
          <span className="ml-auto flex shrink-0 items-center gap-1 text-[11px] text-subtle">
            {run.kind === "load" ? <Gauge size={12} /> : <ListChecks size={12} />}
            {when(run.startedAt ?? 0)}
          </span>
        </div>
        <div className="mt-0.5 truncate text-xs text-muted">{run.headline}</div>
        {run.source && <div className="mono mt-0.5 truncate text-[11px] text-subtle">{basename(run.source)}</div>}
      </button>
    </li>
  );
}

function RunDetail({
  id,
  previous,
  onCompare,
  onDelete,
}: {
  id: number;
  previous?: RunSummary;
  onCompare: (before: number) => void;
  onDelete: () => void;
}) {
  const run = useQuery({ queryKey: ["run", id], queryFn: () => unwrap(commands.getRun(id)) });
  const exportTo = useMutation({
    mutationFn: async (format: ReportFormat) => {
      const name = `${slug(run.data?.summary.name ?? "run")}-${id}`;
      const path = await save({
        title: format === "json" ? "Export the run as JSON" : "Export the run as JUnit XML",
        defaultPath: format === "json" ? `${name}.json` : `${name}.xml`,
        filters: [format === "json" ? { name: "JSON", extensions: ["json"] } : { name: "JUnit XML", extensions: ["xml"] }],
      });
      if (!path) return null;
      await unwrap(commands.exportRun(id, path, format));
      return path;
    },
  });
  if (!run.data) return <div className="text-muted">{run.error ? run.error.message : "Loading…"}</div>;
  const { summary, data } = run.data;

  return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <div className="min-w-0">
          <div className="text-[15px] font-semibold">{summary.name}</div>
          <div className="text-xs text-muted">
            {new Date(summary.startedAt ?? 0).toLocaleString()}
            {summary.source && <span className="mono"> · {summary.source}</span>}
          </div>
        </div>
        <div className="ml-auto flex gap-1.5">
          {previous && (
            <ToolButton onClick={() => onCompare(previous.id)} icon={<GitCompareArrows size={13} />}>
              Compare with previous
            </ToolButton>
          )}
          <ToolButton onClick={() => exportTo.mutate("json")} icon={<Download size={13} />}>
            JSON
          </ToolButton>
          <ToolButton onClick={() => exportTo.mutate("junit")} icon={<Download size={13} />}>
            JUnit
          </ToolButton>
          <ToolButton onClick={onDelete} icon={<Trash2 size={13} />}>
            Delete
          </ToolButton>
        </div>
      </div>
      {exportTo.data && <div className="mt-2 text-xs text-ok">Saved to {exportTo.data}</div>}
      {exportTo.error && <div className="mt-2 text-xs text-bad">{exportTo.error.message}</div>}
      {data.kind === "functional" ? <RunResult result={data.result} /> : <LoadResult report={data.result} />}
    </div>
  );
}

function Compare({ before, after }: { before: RunSummary; after: RunSummary }) {
  const diff = useQuery({
    queryKey: ["diff", before.id, after.id],
    queryFn: () => unwrap(commands.diffRuns(before.id, after.id)),
  });
  return (
    <div>
      <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3">
        <RunCard run={before} label="Before" />
        <ArrowRight size={16} className="text-subtle" />
        <RunCard run={after} label="After" />
      </div>
      {diff.error && <div className="mt-4 text-bad">{diff.error.message}</div>}
      {diff.data && <DiffBody diff={diff.data} />}
    </div>
  );
}

function RunCard({ run, label }: { run: RunSummary; label: string }) {
  return (
    <div className="rounded-lg border border-line bg-surface px-3 py-2">
      <div className="flex items-center gap-1.5 text-xs text-subtle">
        {label} · {new Date(run.startedAt ?? 0).toLocaleString()}
      </div>
      <div className="flex items-center gap-1.5 font-medium">
        {run.passed ? <CircleCheck size={14} className="text-ok" /> : <CircleX size={14} className="text-bad" />}
        <span className="truncate">{run.name}</span>
      </div>
      <div className="truncate text-xs text-muted">{run.headline}</div>
    </div>
  );
}

function DiffBody({ diff }: { diff: RunDiff }) {
  if (diff.kind === "mismatch") {
    return (
      <div className="mt-4 rounded-lg border border-line bg-surface px-4 py-3 text-muted">
        One is a functional run and the other a load test, so there is nothing to line up. Pick two of the same kind.
      </div>
    );
  }
  if (diff.kind === "load") return <LoadDiffView diff={diff} />;
  const { steps, broke, fixed } = diff as Extract<RunDiff, { kind: "functional" }>;
  const changed = steps.filter((s) => s.change !== "still_passing" || s.checks.length > 0);
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-3 border-b border-line px-4 py-3">
        <span className="font-semibold">
          {broke > 0 ? `${count(broke, "step")} broke` : fixed > 0 ? "Nothing broke" : "No change in outcome"}
        </span>
        {fixed > 0 && <span className="text-xs text-ok">{count(fixed, "step")} fixed</span>}
        <span className="ml-auto text-xs text-muted">
          {steps.length - changed.length} of {steps.length} steps unchanged
        </span>
      </div>
      <ol className="divide-y divide-line">
        {steps.map((step) => (
          <StepDiffRow key={step.name} step={step} />
        ))}
      </ol>
    </section>
  );
}

const CHANGES: Record<StepChange, { label: string; className: string; icon: React.ReactNode }> = {
  broke: { label: "broke", className: "bg-bad-soft text-bad", icon: <CircleX size={14} className="text-bad" /> },
  fixed: { label: "fixed", className: "bg-ok-soft text-ok", icon: <CircleCheck size={14} className="text-ok" /> },
  still_failing: { label: "still failing", className: "bg-warn-soft text-warn", icon: <CircleX size={14} className="text-warn" /> },
  still_passing: { label: "", className: "", icon: <CircleCheck size={14} className="text-subtle" /> },
  added: { label: "new", className: "bg-accent-soft text-accent", icon: <CirclePlus size={14} className="text-accent" /> },
  removed: { label: "removed", className: "bg-surface-2 text-subtle", icon: <CircleMinus size={14} className="text-subtle" /> },
};

function StepDiffRow({ step }: { step: StepDiff }) {
  const c = CHANGES[step.change];
  const statusChanged = step.beforeStatus !== step.afterStatus;
  return (
    <li className="px-4 py-2">
      <div className="flex items-center gap-2">
        {c.icon}
        <span className={step.change === "still_passing" ? "text-muted" : "font-medium"}>{step.name}</span>
        {c.label && <span className={`rounded px-1.5 text-[11px] font-semibold ${c.className}`}>{c.label}</span>}
        <span className="mono ml-auto flex shrink-0 gap-3 text-xs text-muted">
          {statusChanged && (
            <span>
              {step.beforeStatus ?? "–"} → <span className="text-ink">{step.afterStatus ?? "–"}</span>
            </span>
          )}
          <span>
            {fmt(step.beforeMs)} → <span className="text-ink">{fmt(step.afterMs)}</span>
          </span>
        </span>
      </div>
      {(step.checks.length > 0 || (step.error && step.change !== "still_passing")) && (
        <ul className="ml-6 mt-1 space-y-0.5 text-xs">
          {step.error && <li className="text-bad">{step.error}</li>}
          {step.checks.map((check) => (
            <li key={check.name} className="flex gap-1.5">
              <span className="mono shrink-0 text-subtle">
                {mark(check.before)} → {mark(check.after)}
              </span>
              <span className={check.after === false ? "text-bad" : check.after === true ? "text-ok" : "text-subtle"}>
                {check.name}
                {check.message && <span className="text-muted">: {check.message}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function LoadDiffView({ diff }: { diff: LoadDiff }) {
  const worse = diff.metrics.filter((m) => m.worse).length;
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="flex items-center border-b border-line px-4 py-3">
        <span className="font-semibold">
          {worse > 0 ? `${count(worse, "figure")} got worse` : "No figure got worse by more than 10%"}
        </span>
      </div>
      <table className="w-full text-[13px]">
        <thead className="text-xs text-subtle">
          <tr>
            <th className="px-4 py-1.5 text-left font-medium">Figure</th>
            <th className="px-2 py-1.5 text-right font-medium">Before</th>
            <th className="px-2 py-1.5 text-right font-medium">After</th>
            <th className="px-4 py-1.5 text-right font-medium">Change</th>
          </tr>
        </thead>
        <tbody className="mono whitespace-nowrap text-xs">
          {diff.metrics.map((m) => (
            <MetricRow key={m.name} metric={m} />
          ))}
        </tbody>
      </table>
      {diff.steps.length > 0 && (
        <table className="w-full border-t border-line text-[13px]">
          <thead className="text-xs text-subtle">
            <tr>
              <th className="w-1/2 px-4 py-1.5 text-left font-medium">Step</th>
              <th className="px-2 py-1.5 text-right font-medium">p95</th>
              <th className="px-4 py-1.5 text-right font-medium">Failed</th>
            </tr>
          </thead>
          <tbody className="mono whitespace-nowrap text-xs">
            {diff.steps.map((s) => (
              <tr key={s.name} className="border-t border-line">
                <td className="max-w-0 truncate px-4 py-1.5 font-sans text-[13px]" title={s.name}>
                  {s.name}
                </td>
                <td className="px-2 py-1.5 text-right">
                  {fmt(s.beforeP95)} → <span className="text-ink">{fmt(s.afterP95)}</span>
                </td>
                <td className={`px-4 py-1.5 text-right ${(s.afterFailed ?? 0) > (s.beforeFailed ?? 0) ? "text-bad" : ""}`}>
                  {s.beforeFailed ?? "–"} → {s.afterFailed ?? "–"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

function MetricRow({ metric: m }: { metric: Metric }) {
  const before = m.before ?? 0;
  const after = m.after ?? 0;
  const change = before === 0 ? (after === 0 ? 0 : 100) : ((after - before) / before) * 100;
  const tone = m.worse ? "text-bad" : m.better ? "text-ok" : "text-muted";
  const value = (v: number) =>
    m.unit === "ms" ? fmt(v) : m.unit === "%" ? `${v.toFixed(2)}%` : `${v.toFixed(1)}/s`;
  return (
    <tr className="border-t border-line">
      <td className="px-4 py-1.5 font-sans text-[13px]">{m.name}</td>
      <td className="px-2 py-1.5 text-right text-muted">{value(before)}</td>
      <td className="px-2 py-1.5 text-right">{value(after)}</td>
      <td className={`px-4 py-1.5 text-right font-semibold ${tone}`}>
        {change > 0 ? "+" : ""}
        {change.toFixed(0)}%{m.worse ? " worse" : m.better ? " better" : ""}
      </td>
    </tr>
  );
}

function ToolButton({ onClick, icon, children }: { onClick: () => void; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
    >
      {icon}
      {children}
    </button>
  );
}

function mark(v: boolean | null | undefined) {
  return v == null ? "–" : v ? "✓" : "✗";
}

function fmt(ms: number | null | undefined): string {
  if (ms == null) return "–";
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  if (ms >= 100) return `${Math.round(ms)} ms`;
  return `${ms.toFixed(1)} ms`;
}

function when(ms: number): string {
  const d = new Date(ms);
  const today = new Date().toDateString() === d.toDateString();
  return today
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString([], { month: "short", day: "numeric" });
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "run";
}

function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}
