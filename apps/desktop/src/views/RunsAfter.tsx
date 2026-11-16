import { GitMerge, Split } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { Step } from "../bindings";
import { afterFor, afterNames, declaredAfter } from "../lib/branches";

/**
 * Which steps `steps[index]` runs after, as a chip that opens a checklist of
 * the steps above it. Ticking several makes the step wait for all of them;
 * two steps ticking the same step run side by side, each a branch.
 */
export function RunsAfter({
  steps,
  index,
  onChange,
  compact,
}: {
  steps: Step[];
  index: number;
  onChange: (after: string[] | undefined) => void;
  /** Just the chip's text, for a card's footer. */
  compact?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const escape = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const names = afterNames(steps, index);
  const chosen = new Set(declaredAfter(steps, index));
  const above = steps.slice(0, index);
  const toggle = (j: number) => {
    const next = above.flatMap((s, k) => ((k === j ? !chosen.has(k) : chosen.has(k)) ? [s.name] : []));
    onChange(afterFor(steps, index, next));
  };
  const label =
    names.length === 0 ? "starts with the flow" : names.length === 1 ? `after ${names[0]}` : `after ${names.join(" + ")}`;
  const joins = names.length > 1;

  return (
    <div ref={box} className={`relative ${compact ? "min-w-0" : ""}`}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        title={
          joins
            ? `Waits for all of ${names.join(", ")} before it runs. Click to change.`
            : "Which steps this one runs after. Click to change."
        }
        className={`flex max-w-full items-center gap-1 rounded px-1 leading-5 hover:bg-accent-soft hover:text-accent ${
          steps[index].after != null ? "text-accent" : "text-subtle"
        }`}
      >
        {joins ? <GitMerge size="0.6875rem" className="shrink-0" /> : <Split size="0.6875rem" className="shrink-0 rotate-180" />}
        <span className="truncate">{label}</span>
      </button>
      {open && (
        // On a card it opens upward, over the card, so the diagram's
        // scroll area does not cut it off.
        <div className={`absolute left-0 z-40 w-72 ${compact ? "bottom-full mb-1" : "top-full mt-1"} max-w-[calc(100vw-2rem)] rounded-lg border border-line bg-surface p-2 text-xs shadow-lg`}>
          <p className="font-semibold text-ink">Runs after</p>
          <p className="mb-1.5 text-subtle">
            Tick several to wait for all of them. Steps that tick the same step run at the same time.
          </p>
          {above.length === 0 && <p className="text-subtle">No steps above this one: it starts with the flow.</p>}
          <ul className="max-h-56 space-y-0.5 overflow-y-auto">
            {above.map((s, j) => (
              <li key={j}>
                <label className="flex cursor-pointer items-center gap-1.5 rounded px-1 py-0.5 hover:bg-surface-2">
                  <input
                    type="checkbox"
                    checked={chosen.has(j)}
                    onChange={() => toggle(j)}
                    className="accent-[var(--accent)]"
                  />
                  <span className={`truncate ${s.enabled === false ? "text-subtle line-through" : ""}`}>{s.name}</span>
                </label>
              </li>
            ))}
          </ul>
          <div className="mt-1.5 flex gap-2 border-t border-line pt-1.5">
            <button
              type="button"
              onClick={() => onChange(index === 0 ? undefined : [])}
              className="rounded px-1 text-muted hover:bg-surface-2 hover:text-ink"
              title="Run with the first steps, in parallel with them"
            >
              Start with the flow
            </button>
            {steps[index].after != null && (
              <button
                type="button"
                onClick={() => onChange(undefined)}
                className="ml-auto rounded px-1 text-muted hover:bg-surface-2 hover:text-ink"
                title="Run after the step just above, as steps do by default"
              >
                Reset
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
