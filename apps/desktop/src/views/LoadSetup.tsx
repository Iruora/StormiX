import { Gauge, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import type { LoadProfile, Step, TestFile } from "../bindings";
import { describeLoad } from "../lib/describe";
import { methodText } from "../lib/methods";
import { DEFAULT_LOAD, LoadEditor, ThresholdsEditor } from "./StepsEditor";

/** What the setup dialog hands back when Run is pressed. */
export type LoadSetupChoice = {
  /** The steps to load test, in flow order. */
  included: number[];
  load: LoadProfile;
  thresholds: string[];
};

/**
 * Sets a load test up before it starts: which of the flow's steps to send,
 * the load profile and the thresholds. Run and Cancel both close it; Escape
 * and the backdrop cancel.
 */
export function LoadSetup({
  test,
  excluded,
  onRun,
  onCancel,
}: {
  test: TestFile;
  /** Names of steps left out last time, so the choice carries over. */
  excluded: string[];
  onRun: (choice: LoadSetupChoice) => void;
  onCancel: () => void;
}) {
  const steps = test.flow.steps;
  // Turned off steps are not offered: they stay unchecked and unlisted, but
  // keep their place so indexes still match `flow.steps`.
  const on = steps.map((s) => s.enabled !== false);
  const offered = on.filter(Boolean).length;
  const [checked, setChecked] = useState<boolean[]>(() => steps.map((s, i) => on[i] && !excluded.includes(s.name)));
  const [load, setLoad] = useState<LoadProfile>(test.load ?? DEFAULT_LOAD);
  const [thresholds, setThresholds] = useState<string[]>(test.thresholds ?? []);
  const selected = checked.filter(Boolean).length;
  const all = selected === offered;
  const toggleAll = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (toggleAll.current) toggleAll.current.indeterminate = selected > 0 && !all;
  }, [selected, all]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  const broken = useMemo(() => missingVariables(steps, checked), [steps, checked]);
  const run = () => onRun({ included: checked.flatMap((on, i) => (on ? [i] : [])), load, thresholds });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Set up the load test"
        className="flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-line bg-surface shadow-2xl"
      >
        <header className="flex items-start gap-3 border-b border-line px-5 py-3">
          <Gauge size={18} className="mt-0.5 shrink-0 text-accent" />
          <div className="min-w-0 flex-1">
            <div className="font-semibold">Load test {test.flow.name}</div>
            <div className="text-xs text-muted">{describeLoad(load)}</div>
          </div>
          <button
            type="button"
            aria-label="Cancel"
            onClick={onCancel}
            className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
          >
            <X size={16} />
          </button>
        </header>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
          <section className="rounded-lg border border-line bg-surface">
            <label className="flex items-center gap-2.5 border-b border-line px-3 py-2">
              <input
                ref={toggleAll}
                type="checkbox"
                aria-label="Select all endpoints"
                checked={all}
                onChange={() => setChecked(on.map((o) => o && !all))}
                className="size-3.5 accent-accent"
              />
              <span className="font-medium">Endpoints</span>
              <span className="ml-auto text-xs text-muted">
                {selected} of {offered} selected
                {offered < steps.length && ` · ${steps.length - offered} turned off, not listed`}
              </span>
            </label>
            <ul className="max-h-72 divide-y divide-line overflow-y-auto">
              {steps.map((step, i) => on[i] && (
                <li key={i}>
                  <label className="flex cursor-pointer items-baseline gap-2.5 px-3 py-1.5 hover:bg-surface-2">
                    <input
                      type="checkbox"
                      aria-label={step.name}
                      checked={checked[i]}
                      onChange={() => setChecked(checked.map((on, j) => (j === i ? !on : on)))}
                      className="size-3.5 translate-y-0.5 accent-accent"
                    />
                    <span className={`mono w-14 shrink-0 text-xs font-semibold ${methodText(step.request.method)}`}>
                      {step.request.method}
                    </span>
                    <div className={`min-w-0 flex-1 ${checked[i] ? "" : "opacity-50"}`}>
                      <div className="truncate text-[13px]">{step.name}</div>
                      <div className="mono truncate text-xs text-muted" title={step.request.url}>
                        {step.request.url}
                      </div>
                    </div>
                  </label>
                </li>
              ))}
            </ul>
            {broken.length > 0 && (
              <div className="flex gap-2 border-t border-line bg-warn-soft px-3 py-2 text-xs text-warn">
                <TriangleAlert size={14} className="mt-px shrink-0" />
                <ul className="space-y-0.5 text-ink">
                  {broken.map((b) => (
                    <li key={`${b.step}-${b.variable}`}>
                      <span className="font-medium">{b.step}</span> uses{" "}
                      <span className="mono">{`{{${b.variable}}}`}</span>, which{" "}
                      <span className="font-medium">{b.from}</span> extracts and is left out.
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </section>

          <LoadEditor load={load} required onChange={(next) => next && setLoad(next)} />
          <ThresholdsEditor thresholds={thresholds} onChange={setThresholds} />
          <p className="text-xs text-subtle">
            The load profile and thresholds are saved into the test file. The endpoint choice only applies to this run.
          </p>
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-line px-5 py-3">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-md border border-line px-3 py-1 text-xs text-muted hover:bg-surface-2"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={selected === 0}
            onClick={run}
            className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-white hover:opacity-90 disabled:opacity-50"
          >
            <Gauge size={13} />
            Run
          </button>
        </footer>
      </div>
    </div>
  );
}

/** Selected steps that use a variable only a left-out step extracts. */
function missingVariables(steps: Step[], checked: boolean[]) {
  const found: { step: string; variable: string; from: string }[] = [];
  steps.forEach((source, i) => {
    if (checked[i]) return;
    for (const variable of Object.keys(source.extract ?? {})) {
      // A selected step before it that extracts the same name still provides it.
      const provided = steps.some((s, j) => checked[j] && j < i && variable in (s.extract ?? {}));
      if (provided) continue;
      const use = new RegExp(`\\{\\{\\s*${variable.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\}\\}`);
      steps.forEach((s, j) => {
        if (checked[j] && j > i && use.test(JSON.stringify(s.request))) {
          found.push({ step: s.name, variable, from: source.name });
        }
      });
    }
  });
  return found;
}
