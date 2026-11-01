import { Eye, EyeOff, KeyRound, LoaderCircle, Plus, RotateCcw, Save, SlidersHorizontal, Trash2, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

/**
 * Changes made to an environment for this session only, by variable name:
 * a new value, or `null` when the variable is removed. They apply to runs
 * at once and are never written to disk unless saved.
 */
export type EnvOverrides = Record<string, string | null>;

/** The environment's variables with the session's changes applied. */
export function applyOverrides(variables: Record<string, string>, overrides: EnvOverrides | undefined) {
  const out = { ...variables };
  for (const [name, value] of Object.entries(overrides ?? {})) {
    if (value === null) delete out[name];
    else out[name] = value;
  }
  return out;
}

/**
 * Shows the chosen environment's variables and lets each one be changed.
 * A change is transient at first: it is used by runs straight away and
 * marked "this session". Saving it writes it to the environment's file,
 * which makes it permanent; resetting it goes back to the file's value.
 * Secrets are masked and never saved with a value.
 */
export function EnvEditor({
  name,
  variables,
  secrets,
  overrides,
  secretValues,
  savesTo,
  onOverride,
  onReset,
  onSave,
  onSecret,
  onClose,
}: {
  name: string;
  /** What the environment's file holds. */
  variables: Record<string, string>;
  secrets: string[];
  overrides: EnvOverrides;
  secretValues: Record<string, string>;
  /** Where saving writes, or null when it will ask. */
  savesTo: string | null;
  onOverride: (name: string, value: string | null) => void;
  /** Drops the session's change to one variable, or to all when null. */
  onReset: (name: string | null) => void;
  /** Writes the session's change to these variables, or to all when null, to the file. */
  onSave: (names: string[] | null) => Promise<void>;
  onSecret: (name: string, value: string) => void;
  onClose: () => void;
}) {
  const [reveal, setReveal] = useState(false);
  const [newName, setNewName] = useState("");
  const [newValue, setNewValue] = useState("");
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const names = [...new Set([...Object.keys(variables), ...Object.keys(overrides)])]
    .filter((n) => !secrets.includes(n))
    .sort((a, b) => a.localeCompare(b));
  const changed = Object.keys(overrides).length;
  const trimmed = newName.trim();
  const taken = trimmed !== "" && (trimmed in variables || trimmed in overrides || secrets.includes(trimmed));
  const canAdd = trimmed !== "" && !taken && !/[\s{}]/.test(trimmed);

  const save = async (which: string[] | null) => {
    setSaving(which?.[0] ?? "*");
    setError(null);
    try {
      await onSave(which);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(null);
    }
  };
  const add = () => {
    if (!canAdd) return;
    onOverride(trimmed, newValue);
    setNewName("");
    setNewValue("");
  };
  /** Typing back the file's value is the same as resetting. */
  const change = (n: string, value: string) => {
    if (n in variables && variables[n] === value) onReset(n);
    else onOverride(n, value);
  };
  const file = savesTo?.split(/[\\/]/).slice(-2).join("/");

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Environment ${name}`}
        className="flex max-h-full w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-line bg-surface shadow-2xl"
      >
        <header className="flex items-start gap-3 border-b border-line px-5 py-3">
          <SlidersHorizontal size="1.125rem" className="mt-0.5 shrink-0 text-accent" />
          <div className="min-w-0 flex-1">
            <div className="font-semibold">Environment {name}</div>
            <div className="truncate text-xs text-muted" title={savesTo ?? undefined}>
              {savesTo ? `Saves to ${file}` : "Not saved to a file yet: saving asks where to put it"}
            </div>
          </div>
          <button
            type="button"
            onClick={() => setReveal(!reveal)}
            title={reveal ? "Mask the secrets" : "Show the secrets"}
            aria-label={reveal ? "Mask the secrets" : "Show the secrets"}
            className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
          >
            {reveal ? <EyeOff size="1rem" /> : <Eye size="1rem" />}
          </button>
          <button
            type="button"
            aria-label="Close"
            onClick={onClose}
            className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
          >
            <X size="1rem" />
          </button>
        </header>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-4">
          <p className="text-xs text-muted">
            A change applies to runs at once, for this session only. <Save size="0.75rem" className="inline" /> saves it
            to the file so it stays; <RotateCcw size="0.75rem" className="inline" /> goes back to the file's value.
          </p>
          <section className="rounded-lg border border-line">
            <ul className="divide-y divide-line">
              {names.length === 0 && <li className="px-3 py-2 text-xs text-muted">No variables yet.</li>}
              {names.map((n) => {
                const inFile = n in variables;
                const over = n in overrides;
                const removed = over && overrides[n] === null;
                const value = over ? (overrides[n] ?? variables[n] ?? "") : variables[n];
                return (
                  <li key={n} className={`flex flex-wrap items-center gap-x-2 gap-y-1 px-3 py-1.5 ${over ? "bg-warn-soft/60" : ""}`}>
                    <span
                      className={`mono min-w-0 shrink-0 basis-40 truncate text-xs ${removed ? "text-subtle line-through" : ""}`}
                      title={n}
                    >
                      {n}
                    </span>
                    <input
                      aria-label={`Value of ${n}`}
                      value={value}
                      disabled={removed}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(e) => change(n, e.target.value)}
                      className={`mono min-w-0 flex-1 basis-48 rounded-md border border-line bg-surface-2 px-2 py-1 text-xs ${
                        removed ? "text-subtle line-through" : ""
                      }`}
                    />
                    <span className="flex shrink-0 items-center gap-1">
                      {over && (
                        <span
                          className="rounded bg-warn-soft px-1.5 py-0.5 text-[0.6875rem] text-warn"
                          title={
                            inFile
                              ? `The file has ${JSON.stringify(variables[n])}`
                              : "Not in the file"
                          }
                        >
                          {removed ? "removed this session" : inFile ? "this session" : "new, this session"}
                        </span>
                      )}
                      {over ? (
                        <>
                          <IconButton
                            label={removed ? `Remove ${n} from the file` : `Save ${n} to the file`}
                            busy={saving === n}
                            disabled={saving !== null}
                            onClick={() => save([n])}
                          >
                            <Save size="0.8125rem" />
                          </IconButton>
                          <IconButton
                            label={inFile ? `Go back to the file's value of ${n}` : `Drop ${n}`}
                            onClick={() => onReset(n)}
                          >
                            <RotateCcw size="0.8125rem" />
                          </IconButton>
                        </>
                      ) : (
                        <IconButton label={`Remove ${n} for this session`} onClick={() => onOverride(n, null)}>
                          <Trash2 size="0.8125rem" />
                        </IconButton>
                      )}
                    </span>
                  </li>
                );
              })}
              {secrets.map((n) => (
                <li key={`secret-${n}`} className="flex flex-wrap items-center gap-x-2 gap-y-1 px-3 py-1.5">
                  <span className="mono flex min-w-0 shrink-0 basis-40 items-center gap-1 truncate text-xs" title={n}>
                    <KeyRound size="0.75rem" className="shrink-0 text-muted" />
                    {n}
                  </span>
                  <input
                    aria-label={`Value of the secret ${n}`}
                    type={reveal ? "text" : "password"}
                    value={secretValues[n] ?? ""}
                    placeholder="No value yet"
                    spellCheck={false}
                    autoComplete="off"
                    onChange={(e) => onSecret(n, e.target.value)}
                    className="mono min-w-0 flex-1 basis-48 rounded-md border border-line bg-surface-2 px-2 py-1 text-xs"
                  />
                  <span
                    className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[0.6875rem] text-muted"
                    title="Secret values stay in memory and are never written to a file"
                  >
                    secret, session only
                  </span>
                </li>
              ))}
            </ul>
            <form
              className="flex flex-wrap items-center gap-2 border-t border-line px-3 py-2"
              onSubmit={(e) => {
                e.preventDefault();
                add();
              }}
            >
              <input
                aria-label="New variable name"
                placeholder="new_variable"
                value={newName}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => setNewName(e.target.value)}
                className="mono min-w-0 shrink-0 basis-40 rounded-md border border-line bg-surface-2 px-2 py-1 text-xs"
              />
              <input
                aria-label="New variable value"
                placeholder="value"
                value={newValue}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => setNewValue(e.target.value)}
                className="mono min-w-0 flex-1 basis-48 rounded-md border border-line bg-surface-2 px-2 py-1 text-xs"
              />
              <button
                type="submit"
                disabled={!canAdd}
                title={taken ? `${trimmed} already exists` : "Add it for this session"}
                className="flex shrink-0 items-center gap-1 rounded-md border border-line px-2 py-1 text-xs text-muted hover:bg-surface-2 disabled:opacity-50"
              >
                <Plus size="0.75rem" /> Add
              </button>
            </form>
          </section>
          {error && <div className="rounded-md bg-bad-soft px-3 py-2 text-xs text-bad">{error}</div>}
        </div>

        <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-5 py-3">
          <span className={`mr-auto text-xs ${changed ? "text-warn" : "text-muted"}`}>
            {changed
              ? `${changed} change${changed === 1 ? "" : "s"} for this session only`
              : "Matches the file"}
          </span>
          <button
            type="button"
            disabled={!changed}
            onClick={() => onReset(null)}
            className="flex items-center gap-1.5 rounded-md border border-line px-3 py-1 text-xs text-muted hover:bg-surface-2 disabled:opacity-50"
          >
            <RotateCcw size="0.8125rem" /> Reset all
          </button>
          <button
            type="button"
            disabled={!changed || saving !== null}
            onClick={() => save(null)}
            className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-white hover:opacity-90 disabled:opacity-50"
          >
            {saving === "*" ? <LoaderCircle size="0.8125rem" className="animate-spin" /> : <Save size="0.8125rem" />}
            Save all to file
          </button>
        </footer>
      </div>
    </div>
  );
}

function IconButton({
  label,
  busy,
  disabled,
  onClick,
  children,
}: {
  label: string;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="rounded p-1 text-muted hover:bg-surface hover:text-ink disabled:opacity-50"
    >
      {busy ? <LoaderCircle size="0.8125rem" className="animate-spin" /> : children}
    </button>
  );
}
