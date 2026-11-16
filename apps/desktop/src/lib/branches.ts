import type { Step } from "../bindings";

/**
 * Parallel branches, as the engine reads `after:`. A step without it runs
 * after the step just above it; `after: [a, b]` waits for both; `after: []`
 * starts with the flow. Only steps above a step can be named.
 */

/** Whether any step says `after:`. Without it the flow is a straight line. */
export const hasBranches = (steps: Step[]) => steps.some((s) => s.after != null);

/** The steps `steps[i]` waits for, by index, turned-off steps included. */
export function declaredAfter(steps: Step[], i: number): number[] {
  const after = steps[i].after;
  if (after == null) return i > 0 ? [i - 1] : [];
  const out: number[] = [];
  for (const name of after) {
    let found = -1;
    for (let j = i - 1; j >= 0; j--) if (steps[j].name === name) { found = j; break; }
    if (found >= 0 && !out.includes(found)) out.push(found);
  }
  return out;
}

/** The names a step waits for, implied or written. */
export const afterNames = (steps: Step[], i: number) => declaredAfter(steps, i).map((j) => steps[j].name);

/** Every step `steps[i]` runs after, directly or further up. */
export function ancestors(steps: Step[], i: number): Set<number> {
  const seen = new Set<number>();
  const walk = (k: number) => {
    for (const d of declaredAfter(steps, k)) if (!seen.has(d)) { seen.add(d); walk(d); }
  };
  walk(i);
  return seen;
}

/** Whether the flow is more than one line: some step forks or joins. */
export function branches(steps: Step[]): boolean {
  return steps.some((_, i) => {
    const d = declaredAfter(steps, i);
    return i === 0 ? d.length > 0 : d.length !== 1 || d[0] !== i - 1;
  });
}

/** Steps' `after:` with `from` renamed to `to`, when a step is renamed. */
export function renameRefs(steps: Step[], from: string, to: string): Step[] {
  if (from === to || !steps.some((s) => s.after?.includes(from))) return steps;
  return steps.map((s) => (s.after?.includes(from) ? { ...s, after: s.after.map((n) => (n === from ? to : n)) } : s));
}

/**
 * The steps without `steps[i]`, with what ran after it now running after
 * what it ran after, so taking a step out never cuts a branch loose.
 */
export function withoutStep(steps: Step[], i: number): Step[] {
  const gone = steps[i];
  const own = afterNames(steps, i);
  const next = steps.map((s, j) => {
    if (j === i + 1 && s.after == null) {
      // It ran after the step above it, the removed one. When that one ran
      // after the step above it too, nothing changes.
      return gone.after == null ? s : { ...s, after: own };
    }
    if (j > i && s.after?.includes(gone.name)) {
      const after = s.after.flatMap((n) => (n === gone.name ? own.filter((o) => !s.after!.includes(o)) : [n]));
      return { ...s, after };
    }
    return s;
  });
  next.splice(i, 1);
  return next;
}

/** Steps placed for the Flow tab's diagram. */
export type Placed = {
  /** In `steps`. */
  index: number;
  /** Depth: one more than the deepest step it runs after. */
  row: number;
  /** Column; a chain of steps stays in one column. */
  lane: number;
  parents: number[];
  children: number[];
};

/**
 * Rows by depth, and columns so that each step continues the column of
 * the first step it runs after, if it is that step's first next step.
 * Each column is then one chain, so no two steps share a cell.
 */
export function layout(steps: Step[]): { placed: Placed[]; lanes: number; rows: number } {
  const placed: Placed[] = steps.map((_, i) => ({ index: i, row: 0, lane: 0, parents: declaredAfter(steps, i), children: [] }));
  for (const p of placed) for (const d of p.parents) placed[d].children.push(p.index);
  let lanes = 0;
  for (const p of placed) {
    p.row = p.parents.length ? Math.max(...p.parents.map((d) => placed[d].row)) + 1 : 0;
    const first = p.parents[0];
    if (first != null && placed[first].children[0] === p.index) p.lane = placed[first].lane;
    else p.lane = lanes++;
    lanes = Math.max(lanes, p.lane + 1);
  }
  const rows = placed.length ? Math.max(...placed.map((p) => p.row)) + 1 : 0;
  return { placed, lanes, rows };
}

/**
 * `after` as written for `steps[i]` running after `names`: left out when
 * that is the step just above, which is what no `after:` means.
 */
export function afterFor(steps: Step[], i: number, names: string[]): string[] | undefined {
  if (i === 0 && names.length === 0) return undefined;
  if (i > 0 && names.length === 1 && names[0] === steps[i - 1].name) return undefined;
  return names;
}

/** A new step that runs after `steps[i]`, beside what already does. */
export function branchFrom(steps: Step[], i: number): Step[] {
  const names = new Set(steps.map((s) => s.name));
  let n = steps.length + 1;
  while (names.has(`Step ${n}`)) n++;
  return [
    ...steps,
    {
      name: `Step ${n}`,
      after: [steps[i].name],
      request: { method: "GET", url: "{{baseUrl}}/" },
      assert: [{ status: 200 }],
    },
  ];
}
