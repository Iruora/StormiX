import { ArrowDown, ArrowUp, Copy, Plus, Trash2, X } from "lucide-react";
import { useRef, useState } from "react";

import type { Assertion, Body, Header, KeyValue, LoadProfile, Method, Step, TestFile } from "../bindings";
import { methodText, methodTone } from "../lib/methods";
import { VarField } from "./VarField";

const METHODS: Method[] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const input =
  "rounded-md border border-line bg-surface px-2 py-1 text-[13px] outline-none focus:border-accent";

/**
 * Edits the parsed test file as forms: the flow's steps, the load profile and
 * the thresholds. Every change hands back a whole new file.
 */
export function StepsEditor({ test, onChange }: { test: TestFile; onChange: (next: TestFile) => void }) {
  const steps = test.flow.steps;
  const setSteps = (next: Step[]) => onChange({ ...test, flow: { ...test.flow, steps: next } });
  const move = (from: number, to: number) => {
    const next = [...steps];
    const [step] = next.splice(from, 1);
    next.splice(to, 0, step);
    setSteps(next);
  };

  return (
    <div className="space-y-4 p-4">
      <label className="block">
        <span className="text-xs font-medium text-subtle">Flow name</span>
        <input
          aria-label="Flow name"
          value={test.flow.name}
          onChange={(e) => onChange({ ...test, flow: { ...test.flow, name: e.target.value } })}
          className={`${input} mt-1 w-full font-semibold`}
        />
      </label>

      {(test.flow.set_aside?.length ?? 0) > 0 && (
        <p className="text-xs text-subtle">
          {test.flow.set_aside!.length} more step{test.flow.set_aside!.length === 1 ? " is" : "s are"} set aside, out of
          the flow; see the Flow tab.
        </p>
      )}
      <ol className="space-y-3">
        {steps.map((step, i) => (
          <StepCard
            key={i}
            index={i}
            step={step}
            last={i === steps.length - 1}
            onChange={(s) => setSteps(steps.map((x, j) => (j === i ? s : x)))}
            onMove={(to) => move(i, to)}
            onDuplicate={() => setSteps([...steps.slice(0, i + 1), { ...step, name: `${step.name} (copy)` }, ...steps.slice(i + 1)])}
            onDelete={() => setSteps(steps.filter((_, j) => j !== i))}
          />
        ))}
      </ol>
      <button
        type="button"
        onClick={() =>
          setSteps([
            ...steps,
            { name: `Step ${steps.length + 1}`, request: { method: "GET", url: "{{baseUrl}}/" }, assert: [{ status: 200 }] },
          ])
        }
        className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-dashed border-line py-2 text-[13px] text-muted hover:border-accent hover:text-accent"
      >
        <Plus size={14} /> Add a step
      </button>

      <LoadEditor load={test.load ?? null} onChange={(load) => onChange({ ...test, load })} />
      <ThresholdsEditor
        thresholds={test.thresholds ?? []}
        onChange={(thresholds) => onChange({ ...test, thresholds })}
      />
    </div>
  );
}

function StepCard({
  index,
  step,
  last,
  onChange,
  onMove,
  onDuplicate,
  onDelete,
}: {
  index: number;
  step: Step;
  last: boolean;
  onChange: (next: Step) => void;
  onMove: (to: number) => void;
  onDuplicate: () => void;
  onDelete: () => void;
}) {
  const request = step.request;
  const on = step.enabled !== false;
  const setRequest = (patch: Partial<Step["request"]>) =>
    onChange({ ...step, request: { ...request, ...patch } as Step["request"] });
  const headers = request.headers ?? [];
  const asserts = step.assert ?? [];
  const status = asserts.find((a): a is Assertion & { status: number } => "status" in a && a.status != null);
  const others = asserts.filter((a) => !("status" in a && a.status != null));
  const kept = [
    others.length > 0 && `${others.length} more assertion${others.length === 1 ? "" : "s"}`,
    step.extract && Object.keys(step.extract).length > 0 && `extracts ${Object.keys(step.extract).join(", ")}`,
    (step.scripts?.pre_request || step.scripts?.test) && "scripts",
  ].filter(Boolean);

  return (
    <li
      id={`step-${index}`}
      className={`scroll-mt-4 rounded-lg border bg-surface ${on ? "border-line" : "border-dashed border-line opacity-70"}`}
    >
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="w-5 text-right text-xs text-subtle">{index + 1}</span>
        <input
          aria-label={`Step ${index + 1} name`}
          value={step.name}
          onChange={(e) => onChange({ ...step, name: e.target.value })}
          className="min-w-0 flex-1 bg-transparent font-medium outline-none"
        />
        {!on && <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[11px] text-subtle">turned off, skipped in runs</span>}
        <label className="flex items-center gap-1 text-xs text-subtle" title="Off steps keep their place but are not sent">
          <input
            type="checkbox"
            aria-label="Run this step"
            checked={on}
            onChange={() => onChange({ ...step, enabled: !on })}
            className="accent-[var(--accent)]"
          />
          run
        </label>
        <IconButton label="Move up" disabled={index === 0} onClick={() => onMove(index - 1)}>
          <ArrowUp size={14} />
        </IconButton>
        <IconButton label="Move down" disabled={last} onClick={() => onMove(index + 1)}>
          <ArrowDown size={14} />
        </IconButton>
        <IconButton label="Duplicate" onClick={onDuplicate}>
          <Copy size={14} />
        </IconButton>
        <IconButton label="Delete" onClick={onDelete}>
          <Trash2 size={14} />
        </IconButton>
      </div>
      <div className="space-y-2 px-3 py-2.5">
        <div className="flex gap-2">
          <select
            aria-label="Method"
            value={request.method}
            onChange={(e) => setRequest({ method: e.target.value as Method })}
            className={`${input} mono w-24 font-semibold ${methodTone(request.method)}`}
          >
            {METHODS.map((m) => (
              <option key={m} className={`bg-surface ${methodText(m)}`}>
                {m}
              </option>
            ))}
          </select>
          <VarField
            aria-label="URL"
            value={request.url}
            onChange={(url) => setRequest({ url })}
            wrap="flex-1"
            className={`${input} mono text-xs`}
          />
          <label className="flex items-center gap-1.5 text-xs text-subtle">
            expect
            <input
              aria-label="Expected status"
              inputMode="numeric"
              placeholder="any"
              value={status?.status ?? ""}
              onChange={(e) => {
                const code = Number.parseInt(e.target.value, 10);
                onChange({ ...step, assert: Number.isFinite(code) ? [{ status: code }, ...others] : others });
              }}
              className={`${input} mono w-14 text-xs text-ink`}
            />
          </label>
        </div>

        <Pairs
          label="Headers"
          rows={headers.map((h) => [h.name, h.value])}
          placeholder={["Header", "Value"]}
          onChange={(rows) => setRequest({ headers: rows.map(([name, value]): Header => ({ name, value })) })}
        />
        <BodyEditor body={request.body ?? null} onChange={(body) => setRequest({ body: body ?? undefined })} />
        {kept.length > 0 && (
          <div className="text-xs text-subtle">Also {kept.join(", ")}; edit those in YAML.</div>
        )}
      </div>
    </li>
  );
}

function Pairs({
  label,
  rows,
  placeholder,
  onChange,
}: {
  label: string;
  rows: [string, string][];
  placeholder: [string, string];
  onChange: (rows: [string, string][]) => void;
}) {
  return (
    <div>
      <div className="flex items-center text-xs text-subtle">
        {label}
        <button
          type="button"
          onClick={() => onChange([...rows, ["", ""]])}
          className="ml-1 rounded px-1 hover:bg-surface-2 hover:text-accent"
          aria-label={`Add to ${label.toLowerCase()}`}
        >
          <Plus size={12} />
        </button>
      </div>
      {rows.map(([k, v], i) => (
        <div key={i} className="mt-1 flex gap-1.5">
          <input
            aria-label={placeholder[0]}
            placeholder={placeholder[0]}
            value={k}
            onChange={(e) => onChange(rows.map((r, j) => (j === i ? [e.target.value, r[1]] : r)))}
            className={`${input} mono w-2/5 text-xs`}
          />
          <VarField
            aria-label={placeholder[1]}
            placeholder={placeholder[1]}
            value={v}
            onChange={(value) => onChange(rows.map((r, j) => (j === i ? [r[0], value] : r)))}
            wrap="flex-1"
            className={`${input} mono text-xs`}
          />
          <IconButton label="Remove" onClick={() => onChange(rows.filter((_, j) => j !== i))}>
            <X size={13} />
          </IconButton>
        </div>
      ))}
    </div>
  );
}

type BodyKind = "none" | "json" | "raw" | "form" | "multipart";

const BODY_KINDS: BodyKind[] = ["none", "json", "raw", "form", "multipart"];

function bodyKind(body: Body | null): BodyKind {
  if (!body) return "none";
  if ("json" in body) return "json";
  if ("raw" in body) return "raw";
  if ("form" in body) return "form";
  return "multipart";
}

/**
 * JSON and raw bodies are edited as text; JSON is kept only once it parses.
 * Switching kinds carries the content over where it can (raw text that is
 * JSON becomes a JSON body, and back), and each kind remembers what it held,
 * so trying another kind and coming back loses nothing.
 */
function BodyEditor({ body, onChange }: { body: Body | null; onChange: (body: Body | null) => void }) {
  const kind = bodyKind(body);
  const [draft, setDraft] = useState<string | null>(null);
  const [bad, setBad] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const remembered = useRef<Partial<Record<BodyKind, Body>>>({});
  const text =
    draft ?? (body && "json" in body ? JSON.stringify(body.json, null, 2) : body && "raw" in body ? body.raw ?? "" : "");

  const switchTo = (next: BodyKind) => {
    if (next === kind) return;
    if (body) remembered.current[kind] = body;
    setNotice(null);
    // A JSON body being typed but not parsing yet is still kept, as raw text.
    const current = kind === "json" && bad ? text : kind === "json" || kind === "raw" ? text : null;
    const fields = body && "form" in body ? body.form : body && "multipart" in body ? body.multipart : null;
    let replacement: Body | null;
    if (next === "none") {
      replacement = null;
    } else if (next === "json") {
      replacement = current != null ? parseJson(current) : null;
      if (current != null && current.trim() && !replacement) {
        // Templated JSON like `{"id": {{id}}}` is not JSON until sent; keep it raw.
        setNotice("This body is not valid JSON on its own (often because of unquoted {{variables}}), so it stays raw. Raw bodies are sent as written, with the Content-Type header.");
        return;
      }
      replacement ??= remembered.current.json ?? { json: {} };
    } else if (next === "raw") {
      replacement = current != null ? { raw: current } : remembered.current.raw ?? { raw: fields ? encodeFields(fields) : "" };
    } else {
      const kept = remembered.current[next];
      replacement = kept ?? (next === "form" ? { form: fields ?? [] } : { multipart: fields ?? [] });
    }
    setDraft(null);
    setBad(false);
    onChange(replacement);
  };

  return (
    <div>
      <div className="flex items-center gap-2 text-xs text-subtle">
        Body
        {BODY_KINDS.map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => switchTo(k)}
            className={`rounded px-1.5 ${kind === k ? "bg-accent-soft text-accent" : "hover:bg-surface-2"}`}
          >
            {k}
          </button>
        ))}
        {bad && <span className="text-bad">not valid JSON yet</span>}
      </div>
      {notice && <div className="mt-1 rounded border border-warn/30 bg-warn-soft px-2 py-1 text-xs text-warn">{notice}</div>}
      {(kind === "json" || kind === "raw") && (
        <div className="mt-1">
          <VarField
            multiline
            aria-label="Body"
            rows={Math.min(8, Math.max(2, text.split("\n").length))}
            value={text}
            onChange={(value) => {
              setDraft(value);
              setNotice(null);
              if (kind === "raw") return onChange({ raw: value });
              const parsed = parseJson(value);
              if (parsed) onChange(parsed);
              setBad(!parsed);
            }}
            onBlur={() => !bad && setDraft(null)}
            className={`${input} mono block resize-y text-xs ${bad ? "border-bad" : ""}`}
          />
        </div>
      )}
      {body && ("form" in body || "multipart" in body) && (
        <div className="mt-1">
          <Pairs
            label={"form" in body ? "Form fields" : "Multipart fields"}
            rows={("form" in body ? body.form ?? [] : body.multipart ?? []).map((f) => [f.key, f.value])}
            placeholder={["Field", "Value"]}
            onChange={(rows) => {
              const fields = rows.map(([key, value]) => ({ key, value }));
              onChange("form" in body ? { form: fields } : { multipart: fields });
            }}
          />
        </div>
      )}
    </div>
  );
}

function parseJson(text: string): Body | null {
  if (!text.trim()) return null;
  try {
    return { json: JSON.parse(text) };
  } catch {
    return null;
  }
}

function encodeFields(fields: KeyValue[]): string {
  return fields.map((f) => `${f.key}=${f.value}`).join("&");
}

type LoadKind = "none" | "constant_rate" | "ramping" | "virtual_users";

function loadKind(load: LoadProfile | null): LoadKind {
  if (!load) return "none";
  if ("constant_rate" in load && load.constant_rate) return "constant_rate";
  if ("ramping" in load && load.ramping) return "ramping";
  return "virtual_users";
}

const LOAD_LABELS: Record<LoadKind, string> = {
  none: "None (run once)",
  constant_rate: "Constant rate",
  ramping: "Ramping rate",
  virtual_users: "Virtual users",
};

function LoadEditor({ load, onChange }: { load: LoadProfile | null; onChange: (load: LoadProfile | null) => void }) {
  const kind = loadKind(load);
  const switchTo = (next: LoadKind) => {
    if (next === "none") onChange(null);
    else if (next === "constant_rate") onChange({ constant_rate: { rate: 10, duration: "30s" } });
    else if (next === "ramping")
      onChange({ ramping: { start_rate: 0, stages: [{ duration: "30s", target: 20 }, { duration: "1m", target: 20 }] } });
    else onChange({ virtual_users: { count: 10, duration: "30s", think_time: null } });
  };

  return (
    <section className="rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="font-medium">Load profile</span>
        <select
          aria-label="Load profile"
          value={kind}
          onChange={(e) => switchTo(e.target.value as LoadKind)}
          className={`${input} ml-auto text-xs`}
        >
          {(Object.keys(LOAD_LABELS) as LoadKind[]).map((k) => (
            <option key={k} value={k}>
              {LOAD_LABELS[k]}
            </option>
          ))}
        </select>
      </div>
      <div className="flex flex-wrap items-end gap-3 px-3 py-2.5 text-xs">
        {kind === "none" && <span className="text-subtle">The flow runs once, as a functional test.</span>}
        {load && "constant_rate" in load && load.constant_rate && (
          <>
            <Field label="iterations/s" value={load.constant_rate.rate} number onChange={(v) => onChange({ constant_rate: { ...load.constant_rate!, rate: Number(v) } })} />
            <Field label="for" value={load.constant_rate.duration} onChange={(v) => onChange({ constant_rate: { ...load.constant_rate!, duration: v } })} />
          </>
        )}
        {load && "virtual_users" in load && load.virtual_users && (
          <>
            <Field label="users" value={load.virtual_users.count} number onChange={(v) => onChange({ virtual_users: { ...load.virtual_users!, count: Number(v) } })} />
            <Field label="for" value={load.virtual_users.duration} onChange={(v) => onChange({ virtual_users: { ...load.virtual_users!, duration: v } })} />
            <Field
              label="think time"
              value={load.virtual_users.think_time ?? ""}
              placeholder="none"
              onChange={(v) => onChange({ virtual_users: { ...load.virtual_users!, think_time: v || null } })}
            />
          </>
        )}
        {load && "ramping" in load && load.ramping && (
          <RampEditor ramp={load.ramping} onChange={(ramping) => onChange({ ramping })} />
        )}
      </div>
    </section>
  );
}

type Ramp = NonNullable<Extract<LoadProfile, { ramping: unknown }>["ramping"]>;

function RampEditor({ ramp, onChange }: { ramp: Ramp; onChange: (ramp: Ramp) => void }) {
  const stages = ramp.stages;
  return (
    <div className="w-full space-y-1.5">
      <Field label="start at /s" value={ramp.start_rate ?? 0} number onChange={(v) => onChange({ ...ramp, start_rate: Number(v) })} />
      {stages.map((stage, i) => (
        <div key={i} className="flex items-end gap-3">
          <span className="w-12 pb-1 text-subtle">stage {i + 1}</span>
          <Field label="to /s" value={stage.target} number onChange={(v) => onChange({ ...ramp, stages: stages.map((s, j) => (j === i ? { ...s, target: Number(v) } : s)) })} />
          <Field label="over" value={stage.duration} onChange={(v) => onChange({ ...ramp, stages: stages.map((s, j) => (j === i ? { ...s, duration: v } : s)) })} />
          <IconButton label="Remove stage" disabled={stages.length === 1} onClick={() => onChange({ ...ramp, stages: stages.filter((_, j) => j !== i) })}>
            <X size={13} />
          </IconButton>
        </div>
      ))}
      <button
        type="button"
        onClick={() => onChange({ ...ramp, stages: [...stages, { duration: "30s", target: stages[stages.length - 1]?.target ?? 10 }] })}
        className="flex items-center gap-1 text-subtle hover:text-accent"
      >
        <Plus size={12} /> Add a stage
      </button>
    </div>
  );
}

function Field({
  label,
  value,
  number,
  placeholder,
  onChange,
}: {
  label: string;
  value: string | number;
  number?: boolean;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex flex-col gap-0.5 text-subtle">
      {label}
      <input
        aria-label={label}
        inputMode={number ? "numeric" : undefined}
        placeholder={placeholder}
        value={value}
        onChange={(e) => {
          if (number && !/^\d*\.?\d*$/.test(e.target.value)) return;
          onChange(e.target.value);
        }}
        className={`${input} mono w-20 text-xs text-ink`}
      />
    </label>
  );
}

const THRESHOLD_HINT = "e.g. p95 < 200ms, errors < 1%, checks > 99%, rps > 50";

function ThresholdsEditor({ thresholds, onChange }: { thresholds: string[]; onChange: (t: string[]) => void }) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const t = draft.trim();
    if (t && !thresholds.includes(t)) onChange([...thresholds, t]);
    setDraft("");
  };
  return (
    <section className="rounded-lg border border-line bg-surface">
      <div className="border-b border-line px-3 py-2 font-medium">Thresholds</div>
      <div className="flex flex-wrap items-center gap-1.5 px-3 py-2.5">
        {thresholds.map((t) => (
          <span key={t} className="mono flex items-center gap-1 rounded bg-surface-2 py-0.5 pl-1.5 pr-0.5 text-xs">
            {t}
            <IconButton label={`Remove ${t}`} onClick={() => onChange(thresholds.filter((x) => x !== t))}>
              <X size={12} />
            </IconButton>
          </span>
        ))}
        <input
          aria-label="New threshold"
          placeholder={THRESHOLD_HINT}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          onBlur={add}
          className="mono min-w-48 flex-1 bg-transparent text-xs outline-none placeholder:text-subtle"
        />
      </div>
    </section>
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
