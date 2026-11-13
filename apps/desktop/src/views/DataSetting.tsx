import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Plus, Table2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { commands, type DataSet, type LoadProfile, type Per, type Pick } from "../bindings";
import { dataColumns, dataRows, inlineSet, parseCsv, PERS, PICKS, relativeTo, toCsv } from "../lib/data";

/** One set as edited: CSV text (the variable names, then a value per line), or a linked file. */
type Draft = {
  text: string;
  /** A CSV file read when the test starts, relative to the test file. */
  file: string | null;
  /** Shown beside a linked file, from when it was picked. */
  rows: number | null;
  pick: Pick;
  per: Per;
};

function draftOf(set: DataSet): Draft {
  return {
    text: set.file != null ? "" : toCsv(dataColumns(set), dataRows(set)),
    file: set.file ?? null,
    rows: null,
    pick: set.pick ?? "random",
    per: set.per ?? "iteration",
  };
}

/** The set a draft stands for, or the reason it cannot be one yet. */
function parsed(d: Draft): { set: DataSet; columns: string[]; rows: string[][] } | { error: string } {
  const how = { ...(d.pick !== "random" && { pick: d.pick }), ...(d.per !== "iteration" && { per: d.per }) };
  if (d.file != null) return { set: { file: d.file, ...how }, columns: [], rows: [] };
  const csv = parseCsv(d.text);
  if ("error" in csv) return csv;
  const bad = csv.columns.find((c) => /[{}\s]/.test(c) || c.startsWith("$"));
  if (bad) return { error: `${bad} is not a usable variable name` };
  return { set: inlineSet(csv.columns, csv.rows, d.pick, d.per), ...csv };
}

/**
 * The file's `data`: lists of values that fill `{{name}}` differently from
 * one load test iteration (or virtual user) to the next, typed in, imported
 * from a CSV file or read from one when the test starts.
 */
export function DataSetting({
  value,
  load,
  filePath,
  onChange,
}: {
  value: DataSet[];
  load: LoadProfile;
  /** The test file's path, so a linked CSV file is kept relative to it. */
  filePath: string | null;
  /** The sets, and whether every draft is one (Run waits for it). */
  onChange: (next: DataSet[], valid: boolean) => void;
}) {
  const [drafts, setDrafts] = useState<Draft[]>(() => value.map(draftOf));
  const [importError, setImportError] = useState<string | null>(null);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const results = drafts.map(parsed);
    onChange(
      results.flatMap((r) => ("set" in r ? [r.set] : [])),
      results.every((r) => "set" in r),
    );
    // onChange is a new function each render; drafts is what changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drafts]);

  const users = "virtual_users" in load && load.virtual_users ? load.virtual_users.count : null;
  const set = (i: number, next: Draft | null) =>
    setDrafts((ds) => (next ? ds.map((d, j) => (j === i ? next : d)) : ds.filter((_, j) => j !== i)));

  const importCsv = async () => {
    setImportError(null);
    const picked = await open({ multiple: false, filters: [{ name: "CSV", extensions: ["csv", "txt"] }] });
    if (typeof picked !== "string") return;
    const res = await commands.readCsvFile(picked);
    if (res.status === "error") {
      setImportError(res.error);
      return;
    }
    const { columns, rows } = res.data;
    // Small files are copied in, so the test file carries its data; big
    // ones are read from the CSV when the test starts.
    const link = rows.length > 1000;
    setDrafts((ds) => [
      ...ds,
      {
        text: link ? "" : toCsv(columns, rows),
        file: link ? relativeTo(picked, filePath) : null,
        rows: link ? rows.length : null,
        pick: "random",
        per: "iteration",
      },
    ]);
  };

  return (
    <div className="rounded-lg border border-line bg-surface p-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="flex items-center gap-1.5 text-xs font-medium text-subtle">
          <Table2 size="0.8125rem" />
          Test data
        </span>
        <span className="text-xs text-muted">
          {drafts.length === 0 ? "Every iteration sends the same values" : "Values change from one iteration or user to the next"}
        </span>
        <div className="ml-auto flex gap-1.5">
          <button
            type="button"
            onClick={() => setDrafts((ds) => [...ds, { text: "", file: null, rows: null, pick: "random", per: "iteration" }])}
            className="flex items-center gap-1 rounded-md border border-line px-2 py-0.5 text-xs text-muted hover:border-accent hover:text-accent"
          >
            <Plus size="0.75rem" /> Add a list
          </button>
          <button
            type="button"
            onClick={() => void importCsv()}
            title="Read a CSV file: its first line names the variables, each line after it is one row"
            className="flex items-center gap-1 rounded-md border border-line px-2 py-0.5 text-xs text-muted hover:border-accent hover:text-accent"
          >
            <FileSpreadsheet size="0.75rem" /> Import CSV…
          </button>
        </div>
      </div>
      {importError && <p className="mt-1.5 text-xs text-bad">{importError}</p>}
      {drafts.length === 0 ? (
        <p className="mt-1.5 text-xs text-subtle">
          Add a list of values, like user IDs, and use it as {"{{userId}}"} in the steps: each iteration picks one, so the load spreads over many users instead of one.
        </p>
      ) : (
        <ul className="mt-2 space-y-2">
          {drafts.map((d, i) => (
            <DraftItem key={i} draft={d} users={users} onChange={(next) => set(i, next)} />
          ))}
        </ul>
      )}
    </div>
  );
}

function DraftItem({ draft: d, users, onChange }: { draft: Draft; users: number | null; onChange: (next: Draft | null) => void }) {
  const result = parsed(d);
  const ok = "set" in result;
  const columns = ok ? result.columns : [];
  const rows = ok ? result.rows : [];
  return (
    <li className="rounded-md border border-line bg-surface-2/40 p-2">
      <div className="flex items-start gap-2">
        {d.file != null ? (
          <div className="min-w-0 flex-1 text-xs">
            <div className="flex items-center gap-1.5">
              <FileSpreadsheet size="0.8125rem" className="shrink-0 text-accent" />
              <span className="mono truncate">{d.file}</span>
            </div>
            <div className="mt-0.5 text-subtle">
              {d.rows != null ? `${d.rows.toLocaleString()} rows, ` : ""}read when the test starts. Its first line names the variables.
            </div>
          </div>
        ) : (
          <label className="min-w-0 flex-1">
            <span className="sr-only">Values, the variable name first</span>
            <textarea
              value={d.text}
              spellCheck={false}
              rows={Math.min(8, Math.max(4, d.text.split("\n").length))}
              placeholder={"userId\nuser-1\nuser-2\nuser-3"}
              onChange={(e) => onChange({ ...d, text: e.target.value })}
              className={`mono w-full resize-y rounded-md border bg-surface px-2 py-1 text-xs leading-relaxed ${
                ok || !d.text ? "border-line" : "border-bad"
              }`}
            />
          </label>
        )}
        <button
          type="button"
          aria-label="Remove this list"
          onClick={() => onChange(null)}
          className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
        >
          <X size="0.875rem" />
        </button>
      </div>
      <div className="mt-1 text-xs">
        {ok ? (
          d.file == null && (
            <span className="text-muted">
              {columns.map((c, j) => (
                <span key={c}>
                  {j > 0 && ", "}
                  <span className="mono text-accent">{`{{${c}}}`}</span>
                </span>
              ))}{" "}
              · {rows.length} value{rows.length === 1 ? "" : "s"}
              {columns.length > 1 && " per variable, one row at a time"}
            </span>
          )
        ) : d.text ? (
          <span className="text-bad">{result.error}</span>
        ) : (
          <span className="text-subtle">The first line is the variable name, then one value per line. For several variables, separate them with commas, like a CSV file.</span>
        )}
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <Segmented label="Pick" options={PICKS} value={d.pick} onChange={(pick) => onChange({ ...d, pick })} />
        <Segmented label="New row for" options={PERS} value={d.per} onChange={(per) => onChange({ ...d, per })} />
      </div>
      <Spread draft={d} rows={rows} users={users} />
    </li>
  );
}

/** What the virtual users will get, when that is known before the test. */
function Spread({ draft: d, rows, users }: { draft: Draft; rows: string[][]; users: number | null }) {
  if (d.per === "user" && users == null) {
    return <p className="mt-1 text-xs text-subtle">This load profile starts a new user for every iteration, so each iteration picks a row.</p>;
  }
  if (d.per !== "user" || users == null || rows.length === 0 || d.pick === "random") {
    if (d.pick === "unique") {
      return <p className="mt-1 text-xs text-subtle">The test ends early once every row has been used.</p>;
    }
    return null;
  }
  // In order or each once: user n takes row n, wrapping (or not starting) past the end.
  const counts = new Map<string, number>();
  const started = d.pick === "unique" ? Math.min(users, rows.length) : users;
  for (let u = 0; u < started; u++) {
    const key = rows[u % rows.length].join(", ");
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return (
    <p className="mt-1 text-xs text-muted">
      {users} users:{" "}
      {[...counts].map(([row, n], j) => (
        <span key={row}>
          {j > 0 && " · "}
          <span className="mono text-ink">{row}</span> ×{n}
        </span>
      ))}
      {started < users && <span className="text-warn"> · {users - started} users have no row left and don't start</span>}
    </p>
  );
}

function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string; title: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="text-xs text-subtle">{label}</span>
      <span role="radiogroup" aria-label={label} className="flex rounded-md border border-line p-0.5">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={value === o.value}
            title={o.title}
            onClick={() => onChange(o.value)}
            className={`rounded px-2 py-0.5 text-xs ${
              value === o.value ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-2 hover:text-ink"
            }`}
          >
            {o.label}
          </button>
        ))}
      </span>
    </span>
  );
}
