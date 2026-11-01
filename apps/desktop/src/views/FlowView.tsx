import {
  Archive,
  ArrowDown,
  ArrowUp,
  CircleCheck,
  CircleX,
  Copy,
  CornerUpLeft,
  Database,
  Eraser,
  Flag,
  GripVertical,
  Play,
  Plus,
  Trash2,
  TriangleAlert,
  Undo2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import type { CachedStep, FlowResult, Step, TestFile } from "../bindings";
import { cachedTitle, storedAgo, useResponseCache } from "../lib/cache";
import { methodTone } from "../lib/methods";
import { useVariables, variablesSetBy, variablesUsedBy } from "../lib/variables";
import { stepPassed } from "./RunResult";

type List = "steps" | "aside";
type Place = { list: List; index: number };
type Drag = Place & { x: number; y: number; dx: number; dy: number; width: number; moved: boolean };
type Removed = Place & { step: Step };

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
  const flowRef = useRef<HTMLOListElement>(null);
  const trayRef = useRef<HTMLDivElement>(null);

  const write = (nextSteps: Step[], nextAside: Step[]) =>
    onChange({ ...test, flow: { ...test.flow, steps: nextSteps, set_aside: nextAside } });
  const lists = () => ({ steps: [...steps], aside: [...aside] });

  const move = (from: Place, to: Place) => {
    const next = lists();
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
    const [step] = next[place.list].splice(place.index, 1);
    setRemoved({ ...place, step });
    write(next.steps, next.aside);
  };
  const undoRemove = () => {
    if (!removed) return;
    const next = lists();
    next[removed.list].splice(Math.min(removed.index, next[removed.list].length), 0, removed.step);
    setRemoved(null);
    write(next.steps, next.aside);
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
  const dragged = drag?.moved ? (drag.list === "steps" ? steps : aside)[drag.index] : null;
  const isDragged = (place: Place) => !!dragged && drag!.list === place.list && drag!.index === place.index;
  const gapActive = (list: List, index: number) =>
    !!target && target.list === list && target.index === index && !!drag &&
    !(drag.list === list && (index === drag.index || index === drag.index + 1));

  return (
    // The Set aside tray sits beside the flow when there is room, below it otherwise.
    <div className="@container min-h-full">
      <div className={`flex min-h-full flex-col gap-4 p-4 @2xl:flex-row ${drag?.moved ? "cursor-grabbing select-none" : ""}`}>
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
            <span className="ml-auto text-subtle">Drag the grip to reorder or set aside. Click a step to edit it.</span>
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

          <Terminal label="Start" icon={<Play size="0.6875rem" />} />
          <ol ref={flowRef} className="relative">
            {steps.map((step, i) => (
              <li key={i}>
                <Gap active={gapActive("steps", i)} onAdd={drag ? undefined : () => insert(i)} />
                <FlowNode
                  step={step}
                  index={i}
                  number={steps.slice(0, i + 1).filter(isOn).length}
                  faded={isDragged({ list: "steps", index: i })}
                  result={isOn(step) ? results.get(step.name) : undefined}
                  notes={notes.get(step) ?? []}
                  cached={cache.steps.get(step.name)}
                  last={i === steps.length - 1}
                  onGrip={startDrag({ list: "steps", index: i })}
                  onToggle={() => update({ list: "steps", index: i }, { ...step, enabled: !isOn(step) })}
                  onCacheToggle={() => update({ list: "steps", index: i }, { ...step, cache: !step.cache })}
                  onClearCache={() => cache.clear(step.name)}
                  onMove={(to) => move({ list: "steps", index: i }, { list: "steps", index: to > i ? to + 1 : to })}
                  onDuplicate={() => duplicate({ list: "steps", index: i })}
                  onSetAside={() => move({ list: "steps", index: i }, { list: "aside", index: aside.length })}
                  onDelete={() => remove({ list: "steps", index: i })}
                  onOpen={() => onOpenStep(i)}
                />
              </li>
            ))}
            <li>
              <Gap active={gapActive("steps", steps.length)} onAdd={drag ? undefined : () => insert(steps.length)} />
            </li>
          </ol>
          <Terminal label="End" icon={<Flag size="0.6875rem" />} />
        </section>

        <aside
          ref={trayRef}
          className={`w-full shrink-0 self-start rounded-lg @2xl:w-64 border border-dashed p-3 transition-colors ${
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
    const set = new Set<string>();
    steps.forEach((step, i) => {
      if (!isOn(step)) return;
      const mine: string[] = [];
      for (const name of variablesUsedBy(step)) {
        const info = known.get(name);
        if (set.has(name) || info?.kind === "defined" || info?.kind === "generated") continue;
        const later = steps.slice(i + 1).find((s) => isOn(s) && variablesSetBy(s).includes(name));
        const off = fromOff.get(name);
        const parked = fromAside.get(name);
        if (later) mine.push(`{{${name}}} is set by “${later.name}”, which now runs after this step`);
        else if (off) mine.push(`{{${name}}} comes from “${off.name}”, which is turned off`);
        else if (parked) mine.push(`{{${name}}} comes from “${parked.name}”, which is set aside`);
      }
      if (mine.length) notes.set(step, mine);
      for (const name of variablesSetBy(step)) set.add(name);
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
  last,
  onGrip,
  onToggle,
  onCacheToggle,
  onClearCache,
  onMove,
  onDuplicate,
  onSetAside,
  onDelete,
  onOpen,
}: {
  step: Step;
  index: number;
  number: number;
  faded: boolean;
  result: FlowResult["steps"][number] | undefined;
  notes: string[];
  cached: CachedStep | undefined;
  last: boolean;
  onGrip: (e: React.PointerEvent) => void;
  onToggle: () => void;
  onCacheToggle: () => void;
  onClearCache: () => void;
  onMove: (to: number) => void;
  onDuplicate: () => void;
  onSetAside: () => void;
  onDelete: () => void;
  onOpen: () => void;
}) {
  const on = isOn(step);
  const sets = variablesSetBy(step);
  return (
    <div data-flow-card className={`flex items-stretch ${faded ? "opacity-30" : ""}`}>
      {/* The rail a run follows; turned off steps branch off it. */}
      <div className="relative w-10 shrink-0">
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
          on ? "border-line" : "ml-6 border-dashed border-line bg-surface-2/50"
        }`}
      >
        <div className="flex flex-wrap items-center gap-2 px-2 py-2">
          <Grip onPointerDown={onGrip} />
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
            <span className="mono block truncate text-[0.6875rem] text-subtle">{step.request.url}</span>
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
            <CacheToggle on={!!step.cache} onClick={onCacheToggle} />
            <Toggle on={on} label={on ? "Turn this step off" : "Turn this step on"} onClick={onToggle} />
          </div>
        </div>
        {(sets.length > 0 || notes.length > 0 || cached) && (
          <div className="space-y-1 border-t border-line px-3 py-1.5 text-[0.6875rem]">
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
            {sets.length > 0 && (
              <div className="text-subtle">
                sets{" "}
                {sets.map((n) => (
                  <span key={n} className="mono mr-1 rounded bg-surface-2 px-1 text-muted">
                    {n}
                  </span>
                ))}
              </div>
            )}
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

/** Whether the step reuses its response; stays visible when on, so a cached step stands out. */
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
