import type { DataSet, Per, Pick } from "../bindings";

/** The variables an inline set fills, in column order; empty for a CSV file. */
export function dataColumns(set: DataSet): string[] {
  if (set.variable != null) return [set.variable];
  const columns: string[] = [];
  for (const row of set.rows ?? []) for (const key of Object.keys(row)) if (!columns.includes(key)) columns.push(key);
  return columns.sort();
}

/** An inline set's rows as values in `dataColumns` order; empty for a CSV file. */
export function dataRows(set: DataSet): string[][] {
  if (set.variable != null) return (set.values ?? []).map((v) => [String(v)]);
  const columns = dataColumns(set);
  return (set.rows ?? []).map((row) => columns.map((c) => (row[c] == null ? "" : String(row[c]))));
}

export const PICKS: { value: Pick; label: string; title: string }[] = [
  { value: "random", label: "Random", title: "Any row, at random each time; rows repeat" },
  { value: "sequential", label: "In order", title: "The rows in order, starting over after the last" },
  { value: "unique", label: "Each once", title: "Each row once, in order; the test ends early once every row is used" },
];

export const PERS: { value: Per; label: string; title: string }[] = [
  { value: "iteration", label: "Each iteration", title: "Every run of the flow gets its own row" },
  {
    value: "user",
    label: "Each user",
    title: "Virtual users only: each user picks a row when it starts and keeps it. With other profiles every iteration is a new user.",
  },
];

/** "picked at random each iteration" and the like. */
export function describePick(set: Pick | DataSet, per?: Per): string {
  const pick = typeof set === "string" ? set : (set.pick ?? "random");
  const each = (typeof set === "string" ? per : set.per) === "user" ? "each virtual user" : "each iteration";
  if (pick === "sequential") return `in order for ${each}`;
  if (pick === "unique") return `each used once, by ${each}`;
  return `picked at random for ${each}`;
}

/** CSV text: a header line, then rows; quoted fields may hold commas, quotes ("") and line breaks. */
export function parseCsv(text: string): { columns: string[]; rows: string[][] } | { error: string } {
  const records: { line: number; fields: string[] }[] = [];
  let field = "";
  let record: string[] = [];
  let quoted = false;
  let touched = false;
  let line = 1;
  let start = 1;
  const src = text.replace(/^﻿/, "");
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (quoted) {
      if (c === '"' && src[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') quoted = false;
      else {
        if (c === "\n") line++;
        field += c;
      }
      continue;
    }
    if (c === '"' && field === "") {
      quoted = true;
      touched = true;
    } else if (c === ",") {
      record.push(field);
      field = "";
      touched = true;
    } else if (c === "\r" && src[i + 1] === "\n") {
      // The \n ends the line.
    } else if (c === "\n" || c === "\r") {
      if (touched || field !== "") {
        record.push(field);
        records.push({ line: start, fields: record });
      }
      field = "";
      record = [];
      touched = false;
      line++;
      start = line;
    } else {
      field += c;
      touched = true;
    }
  }
  if (quoted) return { error: `line ${start}: a quoted field is never closed` };
  if (touched || field !== "") {
    record.push(field);
    records.push({ line: start, fields: record });
  }
  const [header, ...body] = records;
  if (!header) return { error: "the first line names the variables" };
  const columns = header.fields.map((h) => h.trim());
  const blank = columns.findIndex((c) => c === "");
  if (blank >= 0) return { error: `line 1: column ${blank + 1} has no name` };
  const twice = columns.find((c, i) => columns.indexOf(c) !== i);
  if (twice) return { error: `line 1: two columns are named ${twice}` };
  const bad = body.find((r) => r.fields.length !== columns.length);
  if (bad) return { error: `line ${bad.line}: ${bad.fields.length} fields, but the header names ${columns.length}` };
  if (body.length === 0) return { error: "add one value per line under the variable name" };
  return { columns, rows: body.map((r) => r.fields) };
}

/** One CSV field, quoted when it needs to be. */
function csvField(v: string): string {
  return /[",\r\n]/.test(v) || v !== v.trim() ? `"${v.replace(/"/g, '""')}"` : v;
}

/** An inline set as CSV text: the variable names, then one row per line. */
export function toCsv(columns: string[], rows: string[][]): string {
  return [columns, ...rows].map((r) => r.map(csvField).join(",")).join("\n");
}

/** An inline set from columns and rows: one variable as `values`, several as `rows`. */
export function inlineSet(columns: string[], rows: string[][], pick: Pick, per: Per): DataSet {
  const how = { ...(pick !== "random" && { pick }), ...(per !== "iteration" && { per }) };
  if (columns.length === 1) return { variable: columns[0], values: rows.map((r) => r[0]), ...how };
  return { rows: rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, r[i] ?? ""]))), ...how };
}

function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return path.slice(0, cut + 1);
}

function isAbsolute(path: string): boolean {
  return path.startsWith("/") || path.startsWith("\\") || /^[A-Za-z]:[\\/]/.test(path);
}

/** `path` relative to the folder of the file `base` when it is inside it, else as is. */
export function relativeTo(path: string, base: string | null): string {
  if (!base) return path;
  const dir = folderOf(base);
  return dir && path.startsWith(dir) ? path.slice(dir.length).split("\\").join("/") : path;
}

/**
 * The sets with linked CSV files still pointing at the same files once the
 * test file moves from `from` to `to` (Save As), or null when none needs it.
 */
export function rebaseLinkedFiles(data: DataSet[], from: string | null, to: string): DataSet[] | null {
  if (!from || folderOf(from) === folderOf(to)) return null;
  let changed = false;
  const next = data.map((set) => {
    if (set.file == null || isAbsolute(set.file)) return set;
    const sep = from.includes("\\") ? "\\" : "/";
    const absolute = folderOf(from) + set.file.split("/").join(sep);
    changed = true;
    return { ...set, file: relativeTo(absolute, to) };
  });
  return changed ? next : null;
}
