import { Flag, Play } from "lucide-react";
import { useLayoutEffect, useMemo, useRef, useState } from "react";

import type { Step } from "../bindings";
import { layout } from "../lib/branches";

type Rect = { x: number; y: number; w: number; h: number };
type Key = number | "start" | "end";

/** Space between a card and the bar under or over it. */
const GAP = 16;
/** How far a bar reaches past the outermost line. */
const BAR_PAD = 10;

/**
 * A flow with parallel branches, drawn GRAFCET style: steps in rows by how
 * deep they are, a chain of steps in one column, a double bar where
 * branches split and another where they join. Lines are drawn under the
 * cards from where the cards land, so they follow any card height.
 */
export function FlowDiagram({
  steps,
  renderCard,
}: {
  steps: Step[];
  renderCard: (index: number) => React.ReactNode;
}) {
  const { placed, lanes, rows } = useMemo(() => layout(steps), [steps]);
  const box = useRef<HTMLDivElement>(null);
  const [rects, setRects] = useState<Map<Key, Rect>>(new Map());
  const [size, setSize] = useState({ w: 0, h: 0 });

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const base = el.getBoundingClientRect();
      const next = new Map<Key, Rect>();
      el.querySelectorAll<HTMLElement>("[data-node]").forEach((n) => {
        const r = n.getBoundingClientRect();
        const key = n.dataset.node!;
        next.set(key === "start" || key === "end" ? key : Number(key), {
          x: r.left - base.left,
          y: r.top - base.top,
          w: r.width,
          h: r.height,
        });
      });
      setRects(next);
      setSize({ w: el.scrollWidth, h: el.scrollHeight });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    el.querySelectorAll("[data-node]").forEach((n) => observer.observe(n));
    return () => observer.disconnect();
  }, [steps, placed]);

  // Who comes before and after whom, with Start and End as steps too.
  const parents = new Map<Key, Key[]>();
  const children = new Map<Key, Key[]>();
  const link = (p: Key, c: Key) => {
    parents.set(c, [...(parents.get(c) ?? []), p]);
    children.set(p, [...(children.get(p) ?? []), c]);
  };
  for (const p of placed) {
    if (p.parents.length === 0) link("start", p.index);
    for (const d of p.parents) link(d, p.index);
  }
  for (const p of placed) if (p.children.length === 0) link(p.index, "end");

  const firstRoot = placed.find((p) => p.parents.length === 0);
  const lastStep = placed[placed.length - 1];

  type Bar = { key: string; x1: number; x2: number; y: number };
  type Edge = { key: string; from: { x: number; y: number }; to: { x: number; y: number }; y1: number; own: string[] };
  const edges: Edge[] = [];
  const bars: Bar[] = [];
  const cx = (r: Rect) => r.x + r.w / 2;
  for (const [p, cs] of children) {
    const pr = rects.get(p);
    if (!pr) continue;
    const fork = cs.length > 1;
    const forkY = pr.y + pr.h + GAP;
    if (fork) {
      const xs = [cx(pr), ...cs.flatMap((c) => (rects.get(c) ? [cx(rects.get(c)!)] : []))];
      bars.push({ key: `f${p}`, x1: Math.min(...xs), x2: Math.max(...xs), y: forkY });
    }
    for (const c of cs) {
      const cr = rects.get(c);
      if (!cr) continue;
      const join = (parents.get(c)?.length ?? 0) > 1;
      const y1 = fork ? forkY : cr.y - GAP;
      const from = { x: cx(pr), y: pr.y + pr.h };
      const to = { x: cx(cr), y: cr.y };
      edges.push({ key: `${p}-${c}`, from, to, y1, own: [`f${p}`, `j${c}`] });
      if (join && !bars.some((b) => b.key === `j${c}`)) {
        const xs = [cx(cr), ...(parents.get(c) ?? []).flatMap((q) => (rects.get(q) ? [cx(rects.get(q)!)] : []))];
        bars.push({ key: `j${c}`, x1: Math.min(...xs), x2: Math.max(...xs), y: cr.y - GAP });
      }
    }
  }

  // A line that passes over another branch's bar cuts a gap in it, so it
  // does not read as joining or splitting there.
  const gaps: React.ReactNode[] = [];
  for (const e of edges) {
    const runs = [
      { x: e.from.x, a: e.from.y, b: e.y1 },
      { x: e.to.x, a: e.y1, b: e.to.y },
    ];
    for (const r of runs)
      for (const bar of bars) {
        if (e.own.includes(bar.key)) continue;
        if (bar.y > Math.min(r.a, r.b) + 4 && bar.y < Math.max(r.a, r.b) - 4 && r.x >= bar.x1 - BAR_PAD && r.x <= bar.x2 + BAR_PAD)
          gaps.push(<rect key={`${e.key}-${bar.key}-${r.x}`} x={r.x - 6} y={bar.y - 5} width={12} height={10} fill="var(--bg)" />);
      }
  }

  return (
    <div className="overflow-x-auto pb-2">
      <div
        ref={box}
        className="relative grid w-full justify-center"
        style={{
          gridTemplateColumns: `repeat(${lanes}, minmax(16rem, 20rem))`,
          minWidth: `calc(${lanes} * 16rem + ${lanes - 1} * 1.25rem)`,
          columnGap: "1.25rem",
          rowGap: "2.75rem",
        }}
      >
        <svg className="pointer-events-none absolute left-0 top-0" width={size.w} height={size.h} aria-hidden>
          {bars.map((b) => (
            <DoubleBar key={b.key} x1={b.x1} x2={b.x2} y={b.y} />
          ))}
          {gaps}
          {edges.map((e) => (
            <path
              key={e.key}
              d={`M${e.from.x},${e.from.y} V${e.y1} H${e.to.x} V${e.to.y}`}
              fill="none"
              stroke="var(--subtle)"
              strokeOpacity={0.6}
              strokeWidth={2}
            />
          ))}
        </svg>
        <div
          data-node="start"
          className="relative justify-self-center"
          style={{ gridRow: 1, gridColumn: (firstRoot?.lane ?? 0) + 1 }}
        >
          <Terminal label="Start" icon={<Play size="0.6875rem" />} />
        </div>
        {placed.map((p) => (
          <div
            key={p.index}
            data-node={p.index}
            className="relative min-w-0 self-start"
            style={{ gridRow: p.row + 2, gridColumn: p.lane + 1 }}
          >
            {renderCard(p.index)}
          </div>
        ))}
        <div
          data-node="end"
          className="relative justify-self-center"
          style={{ gridRow: rows + 2, gridColumn: (lastStep?.lane ?? 0) + 1 }}
        >
          <Terminal label="End" icon={<Flag size="0.6875rem" />} />
        </div>
      </div>
    </div>
  );
}

/** GRAFCET's double line where branches split or join. */
function DoubleBar({ x1, x2, y }: { x1: number; x2: number; y: number }) {
  const pad = BAR_PAD;
  return (
    <g stroke="var(--accent)" strokeWidth={2}>
      <line x1={x1 - pad} x2={x2 + pad} y1={y - 2.5} y2={y - 2.5} />
      <line x1={x1 - pad} x2={x2 + pad} y1={y + 2.5} y2={y + 2.5} />
    </g>
  );
}

function Terminal({ label, icon }: { label: string; icon: React.ReactNode }) {
  return (
    <div className="flex items-center gap-1.5 rounded-full border border-line bg-surface py-0.5 pl-0.5 pr-2.5">
      <div className="flex size-6 items-center justify-center rounded-full bg-accent text-white">{icon}</div>
      <span className="text-xs font-medium uppercase tracking-wide text-muted">{label}</span>
    </div>
  );
}
