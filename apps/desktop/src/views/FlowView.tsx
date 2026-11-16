import {
  Archive,
  ArrowDown,
  ArrowUp,
  ChevronsDown,
  CircleCheck,
  CircleMinus,
  CircleX,
  Copy,
  CornerUpLeft,
  Database,
  Eraser,
  FileCode2,
  Flag,
  GripVertical,
  OctagonX,
  Play,
  Plus,
  Split,
  Trash2,
  TriangleAlert,
  Undo2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import type { CachedStep, FlowResult, Step, TestFile } from "../bindings";
import { cachedTitle, storedAgo, useResponseCache } from "../lib/cache";
import { methodTone } from "../lib/methods";
import { useVariables, variablesSetBy, variablesUsedBy } from "../lib/variables";
import { ancestors, branchFrom, branches, hasBranches, withoutStep } from "../lib/branches";
import { stepPassed } from "./RunResult";
import { FlowDiagram } from "./FlowDiagram";
import { RunsAfter } from "./RunsAfter";
import { CopyCurlButton } from "./CopyCurl";
import { ScriptsDialog, withScripts } from "./ScriptsDialog";
import { VarText } from "./VarField";

type List = "steps" | "aside";
type Place = { list: List; index: number };
type Drag = Place & { x: number; y: number; dx: number; dy: number; width: number; moved: boolean };
type Removed = Place & { step: Step; before: { steps: Step[]; aside: Step[] } };

const isOn = (step: Step) => step.enabled !== false;

/**
 * The flow as a diagram: enabled steps sit on the line a run follows, turned
 * off steps hang beside it, and set-aside steps wait in a tray, out of the
 * flow. Steps are dragged by their grip, between steps or to and from the
 * tray. Dragging uses pointer events rather than HTML drag and drop, which
 * the desktop webview keeps for files dropped from outside.
 */
export function FlowView({
  test,
  onChange,
  lastRun,
  onOpenStep,
}: {
  test: TestFile;
  onChange: (next: TestFile) => void;
  lastRun: FlowResult | null;
  onOpenStep: (index: number) => void;
}) {
  const steps = test.flow.steps;
  const aside = test.flow.set_aside ?? [];
  const [drag, setDrag] = useState<Drag | null>(null);
  const [target, setTarget] = useState<Place | null>(null);
  const [removed, setRemoved] = useState<Removed | null>(null);
  // The step whose scripts are open in the editor.
  const [scripting, setScripting] = useState<Place | null>(null);
  const flowRef = useRef<HTMLOListElement>(null);
  const trayRef = useRef<HTMLDivElement>(null);

  const stops = test.flow.on_failure !== "continue";
  const setStops = (stop: boolean) =>
    onChange({ ...test, flow: { ...test.flow, on_failure: stop ? undefined : "continue" } });
  const write = (nextSteps: Step[], nextAside: Step[]) =>
    onChange({ ...test, flow: { ...test.flow, steps: nextSteps, set_aside: nextAside } });
  const lists = () => ({ steps: [...steps], aside: [...aside] });

  const move = (from: Place, to: Place) => {
    const next = lists();
    if (from.list === "steps" && to.list === "aside") {
      // The steps after it now run after what it ran after.
      const step = next.steps[from.index];
      next.steps = withoutStep(next.steps, from.index);
      next.aside.splice(to.index, 0, { ...step, after: undefined });
      write(next.steps, next.aside);
      return;
    }
    const [step] = next[from.list].splice(from.index, 1);
    let at = to.index;
    if (from.list === to.list && from.index < at) at -= 1;
    next[to.list].splice(at, 0, step);
    write(next.steps, next.aside);
  };
  const update = (place: Place, step: Step) => {
    const next = lists();
    next[place.list][place.index] = step;
    write(next.steps, next.aside);
  };
  const remove = (place: Place) => {
    const next = lists();
    const step = next[place.list][place.index];
    if (place.list === "steps") next.steps = withoutStep(next.steps, place.index);
    else next.aside.splice(place.index, 1);
    setRemoved({ ...place, step, before: lists() });
    write(next.steps, next.aside);
  };
  const undoRemove = () => {
    if (!removed) return;
    setRemoved(null);
    write(removed.before.steps, removed.before.aside);
  };
  const duplicate = (place: Place) => {
    const next = lists();
    const step = next[place.list][place.index];
    next[place.list].splice(place.index + 1, 0, { ...step, name: `${step.name} (copy)` });
    write(next.steps, next.aside);
  };
  const insert = (index: number) => {
    const next = lists();
    next.steps.splice(index, 0, {
      name: `Step ${steps.length + aside.length + 1}`,
      request: { method: "GET", url: "{{baseUrl}}/" },
      assert: [{ status: 200 }],
    });
    write(next.steps, next.aside);
  };

  // Where a drag would land: a gap in the flow, or the tray.
  const placeAt = (x: number, y: number): Place | null => {
    const tray = trayRef.current?.getBoundingClientRect();
    if (tray && x >= tray.left && x <= tray.right && y >= tray.top && y <= tray.bottom) {
      const cards = [...(trayRef.current?.querySelectorAll("[data-tray-card]") ?? [])];
      const index = cards.filter((c) => {
        const r = c.getBoundingClientRect();
        return r.top + r.height / 2 < y;
      }).length;
      return { list: "aside", index };
    }
    const flow = flowRef.current?.getBoundingClientRect();
    if (!flow || x < flow.left - 40 || x > flow.right + 40) return null;
    const cards = [...(flowRef.current?.querySelectorAll("[data-flow-card]") ?? [])];
    const index = cards.filter((c) => {
      const r = c.getBoundingClientRect();
      return r.top + r.height / 2 < y;
    }).length;
    return { list: "steps", index };
  };

  useEffect(() => {
    if (!drag) return;
    const onMove = (e: PointerEvent) => {
      const moved = drag.moved || Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 4;
      setDrag({ ...drag, x: e.clientX, y: e.clientY, moved });
      setTarget(moved ? placeAt(e.clientX, e.clientY) : null);
    };
    const onUp = (e: PointerEvent) => {
      const to = drag.moved ? placeAt(e.clientX, e.clientY) : null;
      setDrag(null);
      setTarget(null);
      if (to && !(to.list === drag.list && (to.index === drag.index || to.index === drag.index + 1))) {
        move(drag, to);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setDrag(null);
        setTarget(null);
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKey);
    };
  });

  const startDrag = (place: Place) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const card = (e.currentTarget as HTMLElement).closest("[data-flow-card],[data-tray-card]");
    const r = card?.getBoundingClientRect();
    setDrag({
      ...place,
      x: e.clientX,
      y: e.clientY,
      dx: r ? e.clientX - r.left : 0,
      dy: r ? e.clientY - r.top : 0,
      width: r?.width ?? 320,
      moved: false,
    });
  };

  const notes = useDependencyNotes(steps, aside);
  const cache = useResponseCache(test.flow.name);
  const cachedCount = steps.filter((s) => cache.steps.has(s.name)).length;
  const enabled = steps.filter(isOn).length;
  const disabled = steps.length - enabled;
  const results = useMemo(() => new Map(lastRun?.steps.map((s) => [s.name, s]) ?? []), [lastRun]);
  // For each step that failed in the last run, how many steps after it were skipped.
  const stoppedHere = useMemo(() => {
    const counts = new Map<number, number>();
    if (!lastRun) return counts;
    steps.forEach((step, i) => {
      const r = isOn(step) ? results.get(step.name) : undefined;
      if (!r || r.skipped || stepPassed(r)) return;
      const skipped = steps.filter(
        (s, j) => j > i && isOn(s) && results.get(s.name)?.skipped && ancestors(steps, j).has(i),
      ).length;
      if (skipped > 0) counts.set(i, skipped);
    });
    return counts;
  }, [lastRun, results, steps]);
  const branching = useMemo(() => branches(steps), [steps]);
  const showAfter = branching || hasBranches(steps);
  const [view, setView] = useState<"diagram" | "list">("diagram");
  const diagram = branching && view === "diagram";
  const dragged = drag?.moved ? (drag.list === "steps" ? steps : aside)[drag.index] : null;
  const isDragged = (place: Place) => !!dragged && drag!.list === place.list && drag!.index === place.index;
  const gapActive = (list: List, index: number) =>
    !!target && target.list === list && target.index === index && !!drag &&
    !(drag.list === list && (index === drag.index || index === drag.index + 1));

  const card = (step: Step, i: number) => (
      <FlowNode
        step={step}
        index={i}
        number={steps.slice(0, i + 1).filter(isOn).length}
        faded={isDragged({ list: "steps", index: i })}
        result={isOn(step) ? results.get(step.name) : undefined}
        notes={notes.get(step) ?? []}
        cached={cache.steps.get(step.name)}
        flowStops={stops}
        stoppedRun={stoppedHere.get(i) ?? 0}
        last={i === steps.length - 1}
        onGrip={startDrag({ list: "steps", index: i })}
        onToggle={() => update({ list: "steps", index: i }, { ...step, enabled: !isOn(step) })}
        onCacheToggle={() => update({ list: "steps", index: i }, { ...step, cache: !step.cache })}
        onContinueToggle={() =>
          update(
            { list: "steps", index: i },
            { ...step, continue_on_failure: step.continue_on_failure ? undefined : true },
          )
        }
        onClearCache={() => cache.clear(step.name)}
        onScripts={() => setScripting({ list: "steps", index: i })}
        onMove={(to) => move({ list: "steps", index: i }, { list: "steps", index: to > i ? to + 1 : to })}
        onDuplicate={() => duplicate({ list: "steps", index: i })}
        onSetAside={() => move({ list: "steps", index: i }, { list: "aside", index: aside.length })}
        onDelete={() => remove({ list: "steps", index: i })}
        onOpen={() => onOpenStep(i)}
        steps={steps}
        showAfter={showAfter}
        diagram={diagram}
        onAfter={(after) => update({ list: "steps", index: i }, { ...step, after })}
        onBranch={() => write(branchFrom(steps, i), aside)}
      />
  );

  return (
    // The Set aside tray sits beside the flow when there is room, below it otherwise.
    <div className="@container min-h-full">
      {/* Branches need the width, so the tray goes under them. */}
      <div
        className={`flex min-h-full flex-col gap-4 p-4 ${diagram ? "" : "@2xl:flex-row"} ${drag?.moved ? "cursor-grabbing select-none" : ""}`}
      >
        <section className="min-w-0 flex-1">
          <div className="mb-3 flex flex-wrap items-center gap-2 text-xs text-muted">
            <span>
              <b className="text-ink">{enabled}</b> of {steps.length} step{steps.length === 1 ? "" : "s"} run
              {disabled > 0 && <> · {disabled} turned off</>}
              {aside.length > 0 && <> · {aside.length} set aside</>}
            </span>
            {steps.length > 0 && (
              <label className="flex cursor-pointer items-center gap-1.5 rounded border border-line px-1.5 py-0.5 hover:border-accent">
                {/* On only when every step is; turns them all off then, and all on otherwise. */}
                <Toggle
                  on={disabled === 0}
                  label={disabled === 0 ? "Turn every step off" : "Turn every step on"}
                  onClick={() => write(steps.map((s) => ({ ...s, enabled: disabled > 0 })), aside)}
                />
                All steps
              </label>
            )}
            {cachedCount > 0 && (
              <button
                type="button"
                onClick={() => cache.clear(null)}
                title="Forget every cached response in this flow, so the next run sends those requests again"
                className="flex items-center gap-1 rounded border border-line px-1.5 py-0.5 hover:border-accent hover:text-accent"
              >
                <Eraser size="0.6875rem" /> Clear cache ({cachedCount} step{cachedCount === 1 ? "" : "s"})
              </button>
            )}
            <OnFailureSwitch stops={stops} onChange={setStops} />
            {branching && <ViewSwitch view={view} onChange={setView} />}
            <span className="ml-auto text-subtle">
              {diagram
                ? "Steps side by side run at the same time. Click a step to edit it."
                : "Drag the grip to reorder or set aside. Click a step to edit it."}
            </span>
          </div>

          {removed && (
            <div className="mb-3 flex items-center gap-2 rounded-md border border-line bg-surface-2 px-3 py-1.5 text-xs">
              Deleted “{removed.step.name}”.
              <button type="button" onClick={undoRemove} className="flex items-center gap-1 font-medium text-accent">
                <Undo2 size="0.75rem" /> Undo
              </button>
              <button type="button" onClick={() => setRemoved(null)} className="ml-auto text-subtle hover:text-ink">
                Dismiss
              </button>
            </div>
          )}

          {diagram ? (
            <FlowDiagram steps={steps} renderCard={(i) => card(steps[i], i)} />
          ) : (
            <>
            <Terminal label="Start" icon={<Play size="0.6875rem" />} />
            <ol ref={flowRef} className="relative">
              {steps.map((step, i) => (
                <li key={i}>
                  <Gap active={gapActive("steps", i)} onAdd={drag ? undefined : () => insert(i)} />
                  {card(step, i)}
                </li>
              ))}
              <li>
                <Gap active={gapActive("steps", steps.length)} onAdd={drag ? undefined : () => insert(steps.length)} />
              </li>
            </ol>
              <Terminal label="End" icon={<Flag size="0.6875rem" />} />
            </>
          )}
        </section>

        <aside
          ref={trayRef}
          className={`w-full shrink-0 self-start rounded-lg ${diagram ? "" : "@2xl:w-64"} border border-dashed p-3 transition-colors ${
            target?.list === "aside" ? "border-accent bg-accent-soft/40" : "border-line bg-surface-2/40"
          }`}
        >
          <div className="flex items-center gap-1.5 text-xs font-medium text-muted">
            <Archive size="0.8125rem" /> Set aside
            <span className="ml-auto font-normal text-subtle">{aside.length || ""}</span>
          </div>
          <p className="mt-1 text-xs text-subtle">
            Out of the flow, never run, kept in the file. Drag a step here, or back into the flow.
          </p>
          <ul className="mt-2 space-y-1.5">
            {aside.map((step, i) => (
              <li key={i}>
                {gapActive("aside", i) && <DropLine />}
                <div
                  data-tray-card
                  className={`flex items-center gap-1.5 rounded-md border border-line bg-surface px-1.5 py-1.5 ${
                    isDragged({ list: "aside", index: i }) ? "opacity-30" : ""
                  }`}
                >
                  <Grip onPointerDown={startDrag({ list: "aside", index: i })} />
                  <MethodBadge method={step.request.method} />
                  <span className="min-w-0 flex-1 truncate text-xs" title={step.name}>
                    {step.name}
                  </span>
                  <IconButton
                    label="Put back at the end of the flow"
                    onClick={() => move({ list: "aside", index: i }, { list: "steps", index: steps.length })}
                  >
                    <CornerUpLeft size="0.8125rem" />
                  </IconButton>
                  <IconButton label="Delete" onClick={() => remove({ list: "aside", index: i })}>
                    <Trash2 size="0.8125rem" />
                  </IconButton>
                </div>
              </li>
            ))}
            {gapActive("aside", aside.length) && <DropLine />}
          </ul>
        </aside>

        {scripting && (scripting.list === "steps" ? steps : aside)[scripting.index] && (
          <ScriptsDialog
            step={(scripting.list === "steps" ? steps : aside)[scripting.index]}
            onSave={(scripts) =>
              update(scripting, withScripts((scripting.list === "steps" ? steps : aside)[scripting.index], scripts))
            }
            onClose={() => setScripting(null)}
          />
        )}

        {dragged && drag && (
          <div
            className="pointer-events-none fixed z-50 rounded-lg border border-accent bg-surface px-3 py-2 opacity-90 shadow-lg"
            style={{ left: drag.x - drag.dx, top: drag.y - drag.dy, width: Math.min(drag.width, 360) }}
          >
            <div className="flex items-center gap-2">
              <MethodBadge method={dragged.request.method} />
              <span className="truncate font-medium">{dragged.name}</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Warnings for steps whose `{{variables}}` come from a step that will not
 * run before them: one turned off, set aside, or now placed after them.
 */
function useDependencyNotes(steps: Step[], aside: Step[]): Map<Step, string[]> {
  const known = useVariables();
  return useMemo(() => {
    const notes = new Map<Step, string[]>();
    const setters = (list: Step[]) => {
      const by = new Map<string, Step>();
      for (const s of list) for (const name of variablesSetBy(s)) if (!by.has(name)) by.set(name, s);
      return by;
    };
    const fromOff = setters(steps.filter((s) => !isOn(s)));
    const fromAside = setters(aside);
    steps.forEach((step, i) => {
      if (!isOn(step)) return;
      // Only what the steps it runs after set is there when it is sent.
      const before = ancestors(steps, i);
      const set = new Set([...before].filter((j) => isOn(steps[j])).flatMap((j) => variablesSetBy(steps[j])));
      const mine: string[] = [];
      for (const name of variablesUsedBy(step)) {
        const info = known.get(name);
        if (set.has(name) || info?.kind === "defined" || info?.kind === "generated") continue;
        const later = steps.slice(i + 1).find((s, k) => isOn(s) && variablesSetBy(s).includes(name) && ancestors(steps, i + 1 + k).has(i));
        const beside = steps.find((s, j) => j !== i && isOn(s) && !before.has(j) && variablesSetBy(s).includes(name));
        const off = fromOff.get(name);
        const parked = fromAside.get(name);
        if (later) mine.push(`{{${name}}} is set by “${later.name}”, which now runs after this step`);
        else if (beside) mine.push(`{{${name}}} is set by “${beside.name}”, which runs in parallel with this step, so it may not be there yet`);
        else if (off) mine.push(`{{${name}}} comes from “${off.name}”, which is turned off`);
        else if (parked) mine.push(`{{${name}}} comes from “${parked.name}”, which is set aside`);
      }
      if (mine.length) notes.set(step, mine);
    });
    return notes;
  }, [steps, aside, known]);
}

function FlowNode({
  step,
  index,
  number,
  faded,
  result,
  notes,
  cached,
  flowStops,
  stoppedRun,
  last,
  onGrip,
  onToggle,
  onCacheToggle,
  onContinueToggle,
  onClearCache,
  onScripts,
  onMove,
  onDuplicate,
  onSetAside,
  onDelete,
  onOpen,
  steps,
  showAfter,
  diagram,
  onAfter,
  onBranch,
}: {
  step: Step;
  index: number;
  number: number;
  faded: boolean;
  result: FlowResult["steps"][number] | undefined;
  notes: string[];
  cached: CachedStep | undefined;
  /** Whether the flow stops at a failed step. */
  flowStops: boolean;
  /** In the last run, how many steps were skipped after this one failed. */
  stoppedRun: number;
  last: boolean;
  onGrip: (e: React.PointerEvent) => void;
  onToggle: () => void;
  onCacheToggle: () => void;
  onContinueToggle: () => void;
  onClearCache: () => void;
  onScripts: () => void;
  onMove: (to: number) => void;
  onDuplicate: () => void;
  onSetAside: () => void;
  onDelete: () => void;
  onOpen: () => void;
  /** The flow's steps, to pick which ones this one runs after. */
  steps: Step[];
  /** Whether to show which steps this one runs after. */
  showAfter: boolean;
  /** Placed in the branch diagram rather than on the rail. */
  diagram: boolean;
  onAfter: (after: string[] | undefined) => void;
  onBranch: () => void;
}) {
  const on = isOn(step);
  const sets = variablesSetBy(step);
  const keepsGoing = on && flowStops && !!step.continue_on_failure;
  return (
    <div data-flow-card={diagram ? undefined : true} className={`flex items-stretch ${faded ? "opacity-30" : ""}`}>
      {/* The rail a run follows; turned off steps branch off it. */}
      <div className={`relative w-10 shrink-0 ${diagram ? "hidden" : ""}`}>
        <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-line" />
        {on ? (
          <div className="absolute left-1/2 top-1/2 flex size-6 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border-2 border-accent bg-surface text-[0.6875rem] font-semibold text-accent">
            {number}
          </div>
        ) : (
          <>
            <div className="absolute left-1/2 top-1/2 size-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-line" />
            <div className="absolute left-1/2 -right-6 top-1/2 border-t-2 border-dashed border-line" />
          </>
        )}
      </div>
      <div
        className={`group min-w-0 flex-1 rounded-lg border bg-surface transition-colors ${
          on ? "border-line" : `${diagram ? "" : "ml-6"} border-dashed border-line bg-surface-2/50`
        } ${result?.skipped ? "opacity-60" : ""}`}
      >
        <div className="flex flex-wrap items-center gap-2 px-2 py-2">
          {diagram ? (
            <span
              className={`flex size-5 shrink-0 items-center justify-center rounded-full border-2 text-[0.625rem] font-semibold ${
                on ? "border-accent text-accent" : "border-line text-subtle"
              }`}
            >
              {on ? number : "–"}
            </span>
          ) : (
            <Grip onPointerDown={onGrip} />
          )}
          <MethodBadge method={step.request.method} muted={!on} />
          <button
            type="button"
            onClick={onOpen}
            title="Edit this step"
            className={`min-w-0 flex-1 basis-32 text-left ${on ? "" : "text-muted"}`}
          >
            <span className={`block truncate font-medium hover:text-accent ${on ? "" : "line-through decoration-subtle"}`}>
              {step.name || <i className="text-subtle">unnamed</i>}
            </span>
            <VarText text={step.request.url} className="mono block truncate text-[0.6875rem] text-subtle" />
          </button>
          {result && !result.skipped && (
            <span
              className={`flex shrink-0 items-center gap-1 text-xs ${stepPassed(result) ? "text-ok" : "text-bad"}`}
              title="In the last run"
            >
              {stepPassed(result) ? <CircleCheck size="0.8125rem" /> : <CircleX size="0.8125rem" />}
              {result.status ?? "—"} · {result.cached ? "cached" : `${Math.round(result.durationMs ?? 0)} ms`}
            </span>
          )}

          {result?.skipped && (
            <span
              className="flex shrink-0 items-center gap-1 text-xs text-subtle"
              title="In the last run, an earlier step failed and the run stopped before this one"
            >
              <CircleMinus size="0.8125rem" /> skipped, an earlier step failed
            </span>
          )}
          {!on && <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[0.6875rem] text-subtle">skipped</span>}
          {/* The controls move under the name, together, when the card is narrow. */}
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <div className="flex items-center opacity-60 group-hover:opacity-100">
              <IconButton label="Move up" disabled={index === 0} onClick={() => onMove(index - 1)}>
                <ArrowUp size="0.8125rem" />
              </IconButton>
              <IconButton label="Move down" disabled={last} onClick={() => onMove(index + 1)}>
                <ArrowDown size="0.8125rem" />
              </IconButton>
              <IconButton label="Branch: add a step that runs after this one, beside what already does" onClick={onBranch}>
                <Split size="0.8125rem" />
              </IconButton>
              <IconButton label="Duplicate" onClick={onDuplicate}>
                <Copy size="0.8125rem" />
              </IconButton>
              <IconButton label="Set aside, out of the flow" onClick={onSetAside}>
                <Archive size="0.8125rem" />
              </IconButton>
              <IconButton label="Delete" onClick={onDelete}>
                <Trash2 size="0.8125rem" />
              </IconButton>
            </div>
            <div className="opacity-60 group-hover:opacity-100">
              <CopyCurlButton step={step} variant="icon" />
            </div>
            <ScriptsButton step={step} onClick={onScripts} />
            <CacheToggle on={!!step.cache} onClick={onCacheToggle} />
            {flowStops && <ContinueToggle on={!!step.continue_on_failure} onClick={onContinueToggle} />}
            <Toggle on={on} label={on ? "Turn this step off" : "Turn this step on"} onClick={onToggle} />
          </div>
        </div>
        {(showAfter || sets.length > 0 || notes.length > 0 || cached || stoppedRun > 0 || keepsGoing) && (
          <div className="space-y-1 border-t border-line px-3 py-1.5 text-[0.6875rem]">
            {showAfter && (
              <div className="-ml-1 flex">
                <RunsAfter steps={steps} index={index} onChange={onAfter} compact />
              </div>
            )}
            {stoppedRun > 0 && (
              <div className="flex items-center gap-1.5 text-bad">
                <OctagonX size="0.6875rem" className="shrink-0" />
                The last run stopped here: this step failed, so the {stoppedRun === 1 ? "step" : `${stoppedRun} steps`}{" "}
                after it {stoppedRun === 1 ? "was" : "were"} skipped{showAfter ? "; other branches kept going" : ""}
              </div>
            )}
            {keepsGoing && (
              <div className="flex items-center gap-1.5 text-muted">
                <ChevronsDown size="0.6875rem" className="shrink-0" />
                Keeps going: if this step fails, the steps after it still run
              </div>
            )}
            {cached && (
              <div className="flex items-center gap-1.5 text-accent" title={cachedTitle(cached)}>
                <Database size="0.6875rem" className="shrink-0" />
                {cached.responses === 1 ? "Response cached" : `${cached.responses} responses cached`} {storedAgo(cached)};
                runs reuse it instead of sending
                <button
                  type="button"
                  onClick={onClearCache}
                  title="Clear this step's cache, so the next run sends the request again"
                  className="flex items-center gap-0.5 rounded px-1 font-medium hover:bg-accent-soft"
                >
                  <Eraser size="0.6875rem" /> Clear
                </button>
              </div>
            )}
            {sets.length > 0 && <SetsList names={sets} />}
            {notes.map((n) => (
              <div key={n} className="flex items-start gap-1 text-warn">
                <TriangleAlert size="0.75rem" className="mt-px shrink-0" /> {n}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The variables a step sets, as chips that wrap to the card's width. Folded,
 * the list keeps to one line and a button shows how many are hidden; the
 * button opens the full list.
 */
function SetsList({ names }: { names: string[] }) {
  const [open, setOpen] = useState(false);
  const [hidden, setHidden] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // Count the chips that fall below the first line, whenever the card resizes.
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const measure = () => {
      const chips = Array.from(list.children) as HTMLElement[];
      const top = chips[0]?.offsetTop ?? 0;
      setHidden(chips.filter((c) => c.offsetTop > top + 2).length);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    return () => observer.disconnect();
  }, [names]);

  return (
    <div className="flex min-w-0 items-start gap-1.5 text-subtle">
      <span className="shrink-0 leading-5">sets</span>
      <div
        ref={listRef}
        className={`flex min-w-0 flex-1 flex-wrap gap-1 ${open ? "" : "max-h-5 overflow-hidden"}`}
      >
        {names.map((n) => (
          <span
            key={n}
            title={n}
            className="mono max-w-full truncate rounded bg-surface-2 px-1 leading-5 text-muted"
          >
            {n}
          </span>
        ))}
      </div>
      {(hidden > 0 || open) && (
        <button
          type="button"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          title={open ? "Show one line" : `Show all ${names.length} variables`}
          className="shrink-0 whitespace-nowrap rounded px-1 font-medium leading-5 text-accent hover:bg-accent-soft"
        >
          {open ? "Show less" : `+${hidden} more`}
        </button>
      )}
    </div>
  );
}

function Terminal({ label, icon }: { label: string; icon: React.ReactNode }) {
  return (
    <div className="flex items-center">
      <div className="flex w-10 justify-center">
        <div className="flex size-6 items-center justify-center rounded-full bg-accent text-white">{icon}</div>
      </div>
      <span className="text-xs font-medium uppercase tracking-wide text-muted">{label}</span>
    </div>
  );
}

/** The rail between two steps, with a button to add a step there and a line where a drag would drop. */
function Gap({ active, onAdd }: { active: boolean; onAdd?: () => void }) {
  return (
    <div className="group/gap relative flex h-7 items-center">
      <div className="relative h-full w-10 shrink-0">
        <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-line" />
        {onAdd && (
          <button
            type="button"
            aria-label="Add a step here"
            title="Add a step here"
            onClick={onAdd}
            className="absolute left-1/2 top-1/2 flex size-4 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border border-line bg-surface text-subtle opacity-0 hover:border-accent hover:text-accent group-hover/gap:opacity-100 focus:opacity-100"
          >
            <Plus size="0.625rem" />
          </button>
        )}
      </div>
      {active && <DropLine />}
    </div>
  );
}

function DropLine() {
  return <div className="my-0.5 h-0.5 flex-1 rounded bg-accent" />;
}

function Grip({ onPointerDown }: { onPointerDown: (e: React.PointerEvent) => void }) {
  return (
    <span
      role="button"
      aria-label="Drag to move"
      title="Drag to move"
      onPointerDown={onPointerDown}
      className="cursor-grab touch-none rounded p-0.5 text-subtle hover:bg-surface-2 hover:text-ink active:cursor-grabbing"
    >
      <GripVertical size="0.875rem" />
    </span>
  );
}

function MethodBadge({ method, muted }: { method: string; muted?: boolean }) {
  return (
    <span
      className={`mono shrink-0 rounded border px-1 text-[0.625rem] font-semibold ${methodTone(method)} ${
        muted ? "opacity-50" : ""
      }`}
    >
      {method}
    </span>
  );
}

function Toggle({ on, label, onClick }: { on: boolean; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`relative h-4 w-7 shrink-0 rounded-full transition-colors ${on ? "bg-accent" : "bg-line"}`}
    >
      <span
        className={`absolute top-0.5 size-3 rounded-full bg-white shadow transition-all ${on ? "left-3.5" : "left-0.5"}`}
      />
    </button>
  );
}

/** Opens the step's scripts; stays visible when the step has one. */
function ScriptsButton({ step, onClick }: { step: Step; onClick: () => void }) {
  const which = [step.scripts?.pre_request?.trim() && "pre-request", step.scripts?.test?.trim() && "post-response"].filter(
    Boolean,
  );
  const label = which.length ? `Edit the scripts (${which.join(" and ")})` : "Add a pre-request or post-response script";
  return (
    <button
      type="button"
      aria-label="Scripts"
      title={label}
      onClick={onClick}
      className={`flex shrink-0 items-center gap-0.5 rounded p-1 text-[0.6875rem] ${
        which.length
          ? "bg-accent-soft text-accent"
          : "text-subtle opacity-60 hover:bg-surface-2 hover:text-ink group-hover:opacity-100"
      }`}
    >
      <FileCode2 size="0.8125rem" />
      {which.length > 0 && <span className="font-semibold">{which.length === 2 ? "2" : which[0] === "pre-request" ? "pre" : "post"}</span>}
    </button>
  );
}

/** The branch diagram, or the list a drag reorders. */
function ViewSwitch({ view, onChange }: { view: "diagram" | "list"; onChange: (view: "diagram" | "list") => void }) {
  const option = (value: "diagram" | "list", label: string, title: string) => (
    <button
      type="button"
      role="radio"
      aria-checked={view === value}
      title={title}
      onClick={() => onChange(value)}
      className={`rounded px-1.5 py-px ${view === value ? "bg-accent-soft font-medium text-accent" : "hover:text-ink"}`}
    >
      {label}
    </button>
  );
  return (
    <div role="radiogroup" aria-label="Show the flow as" className="flex items-center gap-1 rounded border border-line px-1 py-0.5">
      {option("diagram", "Branches", "Parallel branches side by side, with where they split and join")}
      {option("list", "List", "Every step in file order, to drag into a new order or set aside")}
    </div>
  );
}

/** Whether the step reuses its response; stays visible when on, so a cached step stands out. */
/**
 * Whether a run stops at the first failed step (the default) or runs every
 * step whatever happened before it.
 */
export function OnFailureSwitch({ stops, onChange }: { stops: boolean; onChange: (stops: boolean) => void }) {
  const option = (value: boolean, label: string, title: string) => (
    <button
      type="button"
      role="radio"
      aria-checked={stops === value}
      title={title}
      onClick={() => onChange(value)}
      className={`rounded px-1.5 py-px ${stops === value ? "bg-accent-soft font-medium text-accent" : "hover:text-ink"}`}
    >
      {label}
    </button>
  );
  return (
    <div role="radiogroup" aria-label="When a step fails" className="flex items-center gap-1 rounded border border-line px-1 py-0.5">
      <span className="pl-0.5 text-subtle">When a step fails</span>
      {option(true, "Stop", "Skip the steps after a failed one: they usually need what it was to give them, like a token")}
      {option(false, "Keep going", "Run every step, whatever happened before it")}
    </div>
  );
}

/** Lets one step fail without stopping a flow that stops at failures. */
export function ContinueToggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  const label = on
    ? "Keeps going: if this step fails, the steps after it still run. Click to stop the run at it instead."
    : "Stops the run if it fails. Click to keep going after this step even when it fails.";
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label="Keep going if this step fails"
      title={label}
      onClick={onClick}
      className={`shrink-0 rounded p-1 ${
        on ? "bg-accent-soft text-accent" : "text-subtle opacity-60 hover:bg-surface-2 hover:text-ink group-hover:opacity-100"
      }`}
    >
      <ChevronsDown size="0.8125rem" />
    </button>
  );
}

export function CacheToggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  const label = on
    ? "Caching: runs reuse this step's response. Click to send it every time."
    : "Not cached: sent every time. Click to reuse its response in later runs.";
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label="Cache this step's response"
      title={label}
      onClick={onClick}
      className={`shrink-0 rounded p-1 ${
        on ? "bg-accent-soft text-accent" : "text-subtle opacity-60 hover:bg-surface-2 hover:text-ink group-hover:opacity-100"
      }`}
    >
      <Database size="0.8125rem" />
    </button>
  );
}

function IconButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="rounded p-1 text-subtle hover:bg-surface-2 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent"
    >
      {children}
    </button>
  );
}
