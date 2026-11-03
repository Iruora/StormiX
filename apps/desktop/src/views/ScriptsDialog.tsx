import { CircleAlert, CircleCheck, FileCode2, Keyboard, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { Step } from "../bindings";
import { ScriptEditor, type ScriptEditorHandle, type ScriptStatus } from "../editor/ScriptEditor";
import type { ScriptPhase } from "../editor/scriptComplete";
import { useVariables } from "../lib/variables";

/** The scripts a step keeps: before the request is sent, and after its response (`test` in the file). */
export type StepScripts = { pre_request: string; test: string };

const PHASES: { phase: ScriptPhase; key: keyof StepScripts; label: string; when: string; hint: string }[] = [
  {
    phase: "pre",
    key: "pre_request",
    label: "Pre-request",
    when: "Runs before the request is sent: set headers, compute values, read variables.",
    hint: "// Runs before the request is sent. Try req.setHeader(…) or bru.getVar(…); Ctrl+Space for suggestions.",
  },
  {
    phase: "post",
    key: "test",
    label: "Post-response",
    when: "Runs after the response arrives: save values for later steps, add tests.",
    hint: "// Runs after the response arrives. Try bru.setVar(\"token\", res.body.token) or test(…); Ctrl+Space for suggestions.",
  },
];

const SNIPPETS: Record<ScriptPhase, { label: string; text: string }[]> = {
  pre: [
    { label: "Set a header", text: 'req.setHeader("Authorization", `Bearer ${bru.getVar("token")}`);' },
    { label: "Read a variable", text: 'const value = bru.getVar("name");' },
    { label: "Set a variable", text: 'bru.setVar("name", "value");' },
    { label: "Send a request", text: 'const reply = await bru.sendRequest({ url: "https://…", method: "GET" });' },
  ],
  post: [
    { label: "Save from the response", text: 'bru.setVar("token", res.body.token);' },
    { label: "Add a test", text: 'test("status is 200", () => {\n  expect(res.status).to.equal(200);\n});' },
    { label: "Check a field", text: 'expect(res.body).to.have.property("id");' },
    { label: "Log the body", text: "console.log(res.body);" },
  ],
};

export function scriptsOf(step: Step): StepScripts {
  return { pre_request: step.scripts?.pre_request ?? "", test: step.scripts?.test ?? "" };
}

/** The step with these scripts; empty ones are left out of the file. */
export function withScripts(step: Step, scripts: StepScripts): Step {
  const kept: { pre_request?: string; test?: string } = {};
  if (scripts.pre_request.trim()) kept.pre_request = scripts.pre_request;
  if (scripts.test.trim()) kept.test = scripts.test;
  const { scripts: _, ...rest } = step;
  return Object.keys(kept).length ? { ...rest, scripts: kept } : rest;
}

/**
 * A step's pre-request and post-response scripts in a code editor with
 * completion, in a dialog over the Flow or Steps view. Changes reach the
 * step only when saved; closing with changes asks first.
 */
export function ScriptsDialog({
  step,
  initial = "pre",
  onSave,
  onClose,
}: {
  step: Step;
  initial?: ScriptPhase;
  onSave: (scripts: StepScripts) => void;
  onClose: () => void;
}) {
  const variables = useVariables();
  const original = scriptsOf(step);
  const [scripts, setScripts] = useState(original);
  const [phase, setPhase] = useState<ScriptPhase>(initial);
  const [status, setStatus] = useState<Record<ScriptPhase, ScriptStatus | null>>({ pre: null, post: null });
  const [confirming, setConfirming] = useState(false);
  const editors = useRef<Record<ScriptPhase, ScriptEditorHandle | null>>({ pre: null, post: null });
  const dirty = scripts.pre_request !== original.pre_request || scripts.test !== original.test;
  const errors = (status.pre?.errors ?? 0) + (status.post?.errors ?? 0);

  const save = () => {
    onSave(scripts);
    onClose();
  };
  const requestClose = () => (dirty ? setConfirming(true) : onClose());
  const latest = useRef({ save, requestClose });
  latest.current = { save, requestClose };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Escape first closes the editor's own popups, like the suggestions.
      if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".cm-tooltip-autocomplete")) {
        latest.current.requestClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  useEffect(() => editors.current[phase]?.focus(), [phase]);

  const current = PHASES.find((p) => p.phase === phase)!;
  const here = status[phase];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && requestClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Scripts of ${step.name}`}
        className="flex h-[min(44rem,100%)] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-line bg-surface shadow-2xl"
      >
        <header className="flex items-start gap-3 border-b border-line px-5 py-3">
          <FileCode2 size="1.125rem" className="mt-0.5 shrink-0 text-accent" />
          <div className="min-w-0 flex-1">
            <div className="truncate font-semibold">Scripts · {step.name || "unnamed step"}</div>
            <div className="mono truncate text-xs text-muted">
              {step.request.method} {step.request.url}
            </div>
          </div>
          <button
            type="button"
            aria-label="Close"
            onClick={requestClose}
            className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
          >
            <X size="1rem" />
          </button>
        </header>

        <div className="flex flex-wrap items-end gap-x-1 border-b border-line px-4 pt-2" role="tablist">
          {PHASES.map((p) => {
            const has = scripts[p.key].trim() !== "";
            const bad = (status[p.phase]?.errors ?? 0) > 0;
            return (
              <button
                key={p.phase}
                type="button"
                role="tab"
                aria-selected={phase === p.phase}
                onClick={() => setPhase(p.phase)}
                className={`-mb-px flex items-center gap-1.5 border-b-2 px-3 py-1.5 text-xs font-medium ${
                  phase === p.phase ? "border-accent text-accent" : "border-transparent text-muted hover:text-ink"
                }`}
              >
                {p.label}
                {bad ? (
                  <span className="size-1.5 rounded-full bg-bad" title="Has a syntax error" />
                ) : has ? (
                  <span className="size-1.5 rounded-full bg-accent" title="Has a script" />
                ) : null}
              </button>
            );
          })}
          <span className="ml-auto pb-1.5 text-[0.6875rem] text-subtle">{current.when}</span>
        </div>

        <div className="flex flex-wrap items-center gap-1.5 border-b border-line bg-surface-2/50 px-4 py-1.5 text-[0.6875rem]">
          <span className="text-subtle">Insert</span>
          {SNIPPETS[phase].map((s) => (
            <button
              key={s.label}
              type="button"
              onClick={() => editors.current[phase]?.insert(s.text)}
              title={s.text}
              className="rounded border border-line bg-surface px-1.5 py-0.5 text-muted hover:border-accent hover:text-accent"
            >
              {s.label}
            </button>
          ))}
          <span className="ml-auto flex items-center gap-1 text-subtle">
            <Keyboard size="0.75rem" /> Ctrl+Space suggests · Ctrl+S saves
          </span>
        </div>

        <div className="relative min-h-0 flex-1">
          {PHASES.map((p) => (
            // Both stay mounted, so each keeps its own undo history.
            <div key={p.phase} className={`absolute inset-0 ${phase === p.phase ? "" : "invisible"}`}>
              <ScriptEditor
                ref={(h) => {
                  editors.current[p.phase] = h;
                }}
                value={scripts[p.key]}
                onChange={(text) => setScripts((s) => ({ ...s, [p.key]: text }))}
                phase={p.phase}
                variables={variables}
                onSave={() => latest.current.save()}
                onStatus={(st) => setStatus((all) => ({ ...all, [p.phase]: st }))}
                label={`${p.label} script`}
                hint={p.hint}
              />
            </div>
          ))}
        </div>

        <footer className="flex flex-wrap items-center gap-2 border-t border-line px-5 py-2.5 text-xs">
          {confirming ? (
            <>
              <span className="mr-auto text-warn">Discard your changes to the scripts?</span>
              <button
                type="button"
                onClick={() => setConfirming(false)}
                className="rounded-md border border-line px-3 py-1 text-muted hover:bg-surface-2"
              >
                Keep editing
              </button>
              <button
                type="button"
                onClick={onClose}
                className="rounded-md bg-bad px-3 py-1 font-semibold text-white hover:opacity-90"
              >
                Discard
              </button>
            </>
          ) : (
            <>
              <span className={`flex min-w-0 items-center gap-1.5 ${here?.errors ? "text-bad" : "text-ok"}`}>
                {here?.errors ? <CircleAlert size="0.8125rem" /> : <CircleCheck size="0.8125rem" />}
                <span className="truncate">{here?.errors ? here.first : "No syntax errors"}</span>
              </span>
              {here && <span className="mono shrink-0 text-muted">Ln {here.line}, Col {here.col}</span>}
              <span className="ml-auto shrink-0 text-muted">{dirty ? "Unsaved changes" : ""}</span>
              <button
                type="button"
                onClick={requestClose}
                className="rounded-md border border-line px-3 py-1 text-muted hover:bg-surface-2"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={!dirty}
                onClick={save}
                title={errors ? "Saves anyway; the run reports the error" : undefined}
                className="rounded-md bg-accent px-3 py-1 font-semibold text-white hover:opacity-90 disabled:opacity-50"
              >
                Save scripts
              </button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}
