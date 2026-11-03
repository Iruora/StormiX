import { Check, LoaderCircle, Pencil, RotateCcw, Save, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

import { useVariableEditing, type VariableEditing } from "../lib/variableEdit";
import { describeVariable, type VariableInfo, type VariableTable } from "../lib/variables";

type Place = { name: string; x: number; y: number; above: boolean };
/** Where an edit of a variable goes. */
type Target = "environment" | "secret" | "flow";

/**
 * The hover card for `{{variables}}`: shows the value and where it comes
 * from, and, like Bruno, an Edit button to change the value in place.
 * Returns what a field calls when a variable is entered or left, and the
 * card to render.
 */
export function useVariableTip(variables: VariableTable) {
  const [place, setPlace] = useState<Place | null>(null);
  const [editing, setEditing] = useState(false);
  const hideTimer = useRef<number | undefined>(undefined);

  const cancelHide = () => window.clearTimeout(hideTimer.current);
  const close = () => {
    cancelHide();
    setEditing(false);
    setPlace(null);
  };
  const show = (name: string, el: HTMLElement) => {
    cancelHide();
    // While a value is being typed, other variables do not take the card away.
    if (editing) return;
    const r = el.getBoundingClientRect();
    // Near the bottom of the window, the card opens upwards.
    const above = r.bottom + 120 > window.innerHeight;
    setPlace({ name, x: r.left, y: above ? r.top - 4 : r.bottom + 4, above });
  };
  // A short delay lets the pointer cross the gap to the card and press Edit.
  const leave = () => {
    if (editing) return;
    cancelHide();
    hideTimer.current = window.setTimeout(() => setPlace(null), 220);
  };
  useEffect(() => cancelHide, []);

  const element =
    place &&
    createPortal(
      <VariableCard
        key={place.name}
        place={place}
        info={variables.get(place.name)}
        editing={editing}
        onEdit={() => setEditing(true)}
        onEnter={cancelHide}
        onLeave={leave}
        onClose={close}
      />,
      document.body,
    );
  return { show, leave, element };
}

function targetOf(name: string, info: VariableInfo | undefined, edit: VariableEditing | null): Target | null {
  if (!edit) return null;
  if (edit.envName && edit.secrets.includes(name)) return "secret";
  if (info?.kind === "defined") return info.origin ?? null;
  // Not defined anywhere yet: it goes to the environment, or to the flow when there is none.
  if (!info) return edit.envName ? "environment" : "flow";
  return null;
}

function VariableCard({
  place,
  info,
  editing,
  onEdit,
  onEnter,
  onLeave,
  onClose,
}: {
  place: Place;
  info: VariableInfo | undefined;
  editing: boolean;
  onEdit: () => void;
  onEnter: () => void;
  onLeave: () => void;
  onClose: () => void;
}) {
  const edit = useVariableEditing();
  const { name } = place;
  const target = targetOf(name, info, edit);
  const card = useRef<HTMLDivElement>(null);
  const secret = target === "secret";

  // While editing, a press outside the card or Escape closes it.
  useEffect(() => {
    if (!editing) return;
    const onDown = (e: MouseEvent) => {
      if (card.current && !card.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [editing, onClose]);

  const [first, ...more] = describeVariable(name, info).split("\n");
  const left = Math.max(8, Math.min(place.x, window.innerWidth - cardWidth(editing)));

  return (
    <div
      ref={card}
      role={editing ? "dialog" : "tooltip"}
      aria-label={editing ? `Edit {{${name}}}` : undefined}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      // The card is portalled, but React still bubbles its events to the
      // field's parents, like the Flow card's "open this step" button.
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      className={`fixed z-[60] rounded-md border border-line bg-surface px-2.5 py-1.5 text-xs shadow-lg ${
        editing ? "w-[min(26rem,calc(100vw-1rem))]" : "max-w-[min(24rem,calc(100vw-1rem))]"
      }`}
      style={{ left, top: place.y, transform: place.above ? "translateY(-100%)" : undefined }}
    >
      {editing && target && edit ? (
        <EditValue name={name} info={info} target={target} edit={edit} onDone={onClose} />
      ) : (
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            {info?.value != null ? (
              <>
                <div className="mono break-all">
                  <Name name={name} info={info} />
                  <span className="text-subtle"> = </span>
                  <span className="text-ink">{secret ? "••••••" : info.value}</span>
                </div>
                <div className="mt-0.5 text-subtle">{more.join(" ")}</div>
              </>
            ) : (
              <div className={info ? "text-muted" : "text-warn"}>{first}</div>
            )}
          </div>
          {target && (
            <button
              type="button"
              onClick={onEdit}
              title={info ? `Change the value of {{${name}}}` : `Give {{${name}}} a value`}
              className="flex shrink-0 items-center gap-1 rounded border border-line px-1.5 py-0.5 font-medium text-accent hover:border-accent hover:bg-accent-soft"
            >
              <Pencil size="0.6875rem" /> {info ? "Edit" : "Set"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function Name({ name, info }: { name: string; info: VariableInfo | undefined }) {
  const tone = !info ? "var-missing" : info.kind === "generated" ? "var-generated" : info.kind === "runtime" ? "var-runtime" : "var-defined";
  return <span className={tone}>{`{{${name}}}`}</span>;
}

/**
 * The value as a field. Enter applies it for this session (an environment
 * value or secret) or writes it into the test file (a flow variable);
 * "Save to file" makes an environment value permanent.
 */
function EditValue({
  name,
  info,
  target,
  edit,
  onDone,
}: {
  name: string;
  info: VariableInfo | undefined;
  target: Target;
  edit: VariableEditing;
  onDone: () => void;
}) {
  const [value, setValue] = useState(info?.value ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, []);

  const inFile = name in edit.fileValues;
  const changedThisSession = target === "environment" && name in edit.overrides;
  const apply = () => {
    if (target === "secret") edit.setSecret(name, value);
    else if (target === "flow") edit.setFlowVar(name, value);
    // Typing back the file's value is the same as resetting, as in the Variables dialog.
    else if (inFile && edit.fileValues[name] === value) edit.resetEnvVar(name);
    else edit.setEnvVar(name, value);
    onDone();
  };
  const saveToFile = async () => {
    setSaving(true);
    setError(null);
    try {
      edit.setEnvVar(name, value);
      await edit.saveEnvVar(name, value);
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  const where: Record<Target, ReactNode> = {
    environment: (
      <>
        environment <b className="font-medium text-ink">{edit.envName}</b>
        {changedThisSession && <span className="ml-1 rounded bg-warn-soft px-1 text-warn">this session</span>}
      </>
    ),
    secret: <>secret in {edit.envName}, kept in memory only</>,
    flow: <>the flow's variables, in this test file</>,
  };
  const file = edit.savesTo?.split(/[\\/]/).slice(-2).join("/");

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        apply();
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") onDone();
      }}
      className="space-y-1.5 py-0.5"
    >
      <div className="flex items-center gap-1.5">
        <span className="mono min-w-0 truncate">
          <Name name={name} info={info} />
        </span>
        <span className="min-w-0 truncate text-subtle">in {where[target]}</span>
        <button
          type="button"
          aria-label="Cancel"
          title="Cancel (Esc)"
          onClick={onDone}
          className="ml-auto shrink-0 rounded p-0.5 text-subtle hover:bg-surface-2 hover:text-ink"
        >
          <X size="0.75rem" />
        </button>
      </div>
      <input
        ref={field}
        aria-label={`Value of ${name}`}
        type={target === "secret" ? "password" : "text"}
        value={value}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => setValue(e.target.value)}
        className="mono w-full rounded-md border border-line bg-surface-2 px-2 py-1 text-xs outline-none focus:border-accent"
      />
      {error && <div className="rounded bg-bad-soft px-2 py-1 text-bad">{error}</div>}
      <div className="flex flex-wrap items-center gap-1.5">
        <button
          type="submit"
          title={
            target === "flow"
              ? "Change it in the test file; save the file to keep it"
              : "Use it in runs now, for this session only"
          }
          className="flex items-center gap-1 rounded-md bg-accent px-2 py-0.5 font-semibold text-white hover:opacity-90"
        >
          <Check size="0.75rem" /> {target === "flow" ? "Apply" : "Use for this session"}
        </button>
        {target === "environment" && (
          <button
            type="button"
            disabled={saving}
            onClick={saveToFile}
            title={edit.savesTo ? `Write it to ${edit.savesTo}` : "Write it to the environment's file; asks where first"}
            className="flex items-center gap-1 rounded-md border border-line px-2 py-0.5 text-muted hover:border-accent hover:text-accent disabled:opacity-50"
          >
            {saving ? <LoaderCircle size="0.75rem" className="animate-spin" /> : <Save size="0.75rem" />} Save to file
          </button>
        )}
        {changedThisSession && inFile && (
          <button
            type="button"
            onClick={() => {
              edit.resetEnvVar(name);
              onDone();
            }}
            title={`Go back to the file's value: ${edit.fileValues[name]}`}
            className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-muted hover:bg-surface-2 hover:text-ink"
          >
            <RotateCcw size="0.75rem" /> Reset
          </button>
        )}
        <span className="ml-auto truncate text-[0.6875rem] text-subtle" title={edit.savesTo ?? undefined}>
          {target === "environment" && file ? file : target === "secret" ? "never saved" : ""}
        </span>
      </div>
    </form>
  );
}

/** The widest the card gets, in pixels, plus a margin. */
function cardWidth(editing: boolean): number {
  const rem = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  return Math.min((editing ? 26 : 24) * rem, window.innerWidth - 16) + 8;
}
