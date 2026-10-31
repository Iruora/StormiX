import type { Diagnostic } from "@codemirror/lint";
import { isMap, isPair, isScalar, isSeq, parseDocument, type Node as YamlNode } from "yaml";

import type { Issue, Validation } from "../bindings";

/**
 * Problems in the text, placed where they are. Syntax errors come from
 * parsing here, on every keystroke; format errors and warnings come from
 * the Rust validator, whose paths like `flow.steps[1].request.url` are
 * found in the document.
 */
export function diagnose(text: string, validation: Validation | undefined): Diagnostic[] {
  const doc = parseDocument(text, { prettyErrors: false, uniqueKeys: true });
  const syntax: Diagnostic[] = [];
  const lines = new Set<number>();
  for (const e of [...doc.errors, ...doc.warnings]) {
    // One mistake often confuses the parser for the lines after it; the
    // first problem on each line, and only the first few, are the useful ones.
    const line = lineOf(text, e.pos[0]);
    if (lines.has(line) || lines.size >= MAX_SYNTAX) continue;
    lines.add(line);
    const deeper = e.code === "BAD_INDENT" || /Nested mappings/.test(e.message) ? overIndented(text, e.pos[0]) : null;
    if (deeper) {
      lines.add(lineOf(text, deeper[0]));
      syntax.push({
        from: deeper[0],
        to: deeper[1],
        severity: "error",
        source: "YAML syntax",
        message: "This line is indented deeper than the key above it. Line it up with the keys at its level.",
      });
      continue;
    }
    const [from, to] = visible(text, e.pos[0], e.pos[1]);
    syntax.push({
      from,
      to,
      severity: doc.errors.includes(e as never) ? "error" : "warning",
      source: "YAML syntax",
      message: syntaxMessage(e.code, e.message),
    });
  }
  // Until the syntax is fixed, the validator's view is older than the text.
  if (doc.errors.length > 0 || !validation) return syntax;

  const out = [...syntax];
  if (validation.parseError) {
    out.push(fromParseError(text, doc, validation.parseError));
  }
  for (const issue of validation.issues) out.push(fromIssue(text, doc, issue));
  return out;
}

type Doc = ReturnType<typeof parseDocument>;

const MAX_SYNTAX = 3;

function fromIssue(text: string, doc: Doc, issue: Issue): Diagnostic {
  const [from, to] = locate(text, doc, issue.path);
  return {
    from,
    to,
    severity: issue.severity === "error" ? "error" : "warning",
    source: issue.path,
    message: issue.message,
  };
}

/** `path: message` from the format check, or a serde_yaml syntax error with a line and column. */
function fromParseError(text: string, doc: Doc, error: string): Diagnostic {
  const at = /at line (\d+) column (\d+)/.exec(error);
  if (at) {
    const from = offsetOf(text, Number(at[1]), Number(at[2]));
    return { from, to: lineEnd(text, from), severity: "error", source: "YAML", message: error };
  }
  const split = /^([^\s:]+): (.*)$/s.exec(error);
  if (split) {
    // An unknown key is the key's fault, not its value's.
    const [from, to] = locate(text, doc, split[1], split[2].startsWith("unknown field"));
    return { from, to, severity: "error", source: split[1], message: split[2] };
  }
  return { from: 0, to: lineEnd(text, 0), severity: "error", source: "Format", message: error };
}

/**
 * The range a path points at: a scalar's value, else the key that holds it,
 * so an unknown key or a wrong section is underlined where it is written.
 * A path that goes deeper than the document stops at the last part found.
 */
export function locate(text: string, doc: Doc, path: string, preferKey = false): [number, number] {
  let node: unknown = doc.contents;
  let keyRange: [number, number] | null = null;
  for (const part of parsePath(path)) {
    if (typeof part === "number" && isSeq(node) && part < node.items.length) {
      node = node.items[part];
      keyRange = null;
    } else if (typeof part === "string" && isMap(node)) {
      const pair = node.items.find((p) => isPair(p) && isScalar(p.key) && String(p.key.value) === part);
      if (!pair) break;
      const key = pair.key as YamlNode;
      keyRange = key.range ? [key.range[0], key.range[1]] : null;
      node = pair.value;
      if (node == null) break;
    } else {
      break;
    }
  }
  const range = (node as YamlNode | null)?.range;
  if (preferKey && keyRange) return clamp(text, keyRange[0], keyRange[1]);
  if (range && isScalar(node)) return clamp(text, range[0], range[1]);
  if (keyRange) return clamp(text, keyRange[0], keyRange[1]);
  if (range) return clamp(text, range[0], lineEnd(text, range[0]));
  return [0, lineEnd(text, 0)];
}

/** `flow.steps[1].request` → `["flow", "steps", 1, "request"]`. `.` is the root. */
export function parsePath(path: string): (string | number)[] {
  const parts: (string | number)[] = [];
  for (const m of path.matchAll(/([^.[\]]+)|\[(\d+)\]/g)) {
    parts.push(m[2] != null ? Number(m[2]) : m[1]);
  }
  return parts;
}

/** The yaml package's wording, with a hint for the mistakes people make most. */
function syntaxMessage(code: string, message: string): string {
  // It appends ` at line N, column M:` and a code excerpt; the editor shows both already.
  const base = message
    .replace(/ at line \d+, column \d+:?[\s\S]*$/, "")
    .trim()
    .replace('Missing closing "quote', "Missing closing quote")
    .replace("Missing closing 'quote", "Missing closing quote");
  const hint: Record<string, string> = {
    TAB_AS_INDENT: "Indent with spaces.",
    DUPLICATE_KEY: "This key is already set at this level.",
    MISSING_CHAR: "A quote, bracket or brace is not closed.",
    BAD_INDENT: "Check the indentation: keys at one level line up with each other.",
    BLOCK_AS_IMPLICIT_KEY: "Check the indentation of this line and the one above it.",
    MULTILINE_IMPLICIT_KEY: "A key must fit on one line; a `:` may be missing, or the indentation is off.",
  };
  if (/Nested mappings are not allowed in compact mappings/.test(base)) {
    return `${base}. If the value contains ": ", put it in quotes; otherwise check the indentation.`;
  }
  return hint[code] ? `${base}. ${hint[code]}` : base;
}

/**
 * A range worth underlining: the parser often points at a single character,
 * the end of a line, or a span that runs on for many lines.
 */
function visible(text: string, from: number, to: number): [number, number] {
  const start = text.lastIndexOf("\n", from - 1) + 1;
  const end = lineEnd(text, start);
  let [a, b] = clamp(text, from, Math.min(Math.max(to, from + 1), end));
  if (b <= a || /^\s*$/.test(text.slice(a, b))) {
    // At the end of a line or on blank space: underline the line's content.
    const content = text.slice(start, end);
    const lead = content.length - content.trimStart().length;
    if (content.trim()) return [start + lead, start + content.trimEnd().length];
    return [a, Math.min(a + 1, text.length)];
  }
  if (b - a === 1) {
    // One character: widen it to the word it is in.
    while (b < end && !/[\s,:{}[\]]/.test(text[b])) b++;
  }
  return [a, b];
}

/**
 * `key: value` followed by a more indented `other: x` reads to YAML as a
 * nested map inside the value; the mistake is the second line's indent.
 */
function overIndented(text: string, pos: number): [number, number] | null {
  const start = text.lastIndexOf("\n", pos - 1) + 1;
  const end = lineEnd(text, start);
  const line = text.slice(start, end);
  const own = /^(\s*)(?:-\s+)?[^\s:#][^:#]*:\s+\S/.exec(line);
  if (!own || end >= text.length) return null;
  const nextStart = end + 1;
  const next = text.slice(nextStart, lineEnd(text, nextStart));
  const m = /^(\s*)\S[^:#]*:(\s|$)/.exec(next);
  if (!m || m[1].length <= own[1].length) return null;
  return [nextStart + m[1].length, nextStart + next.trimEnd().length];
}

function lineOf(text: string, pos: number): number {
  let n = 0;
  for (let i = text.indexOf("\n"); i !== -1 && i < pos; i = text.indexOf("\n", i + 1)) n++;
  return n;
}

function offsetOf(text: string, line: number, column: number): number {
  let start = 0;
  for (let i = 1; i < line; i++) {
    const next = text.indexOf("\n", start);
    if (next === -1) return text.length;
    start = next + 1;
  }
  return Math.min(start + column - 1, lineEnd(text, start));
}

function lineEnd(text: string, from: number): number {
  const end = text.indexOf("\n", from);
  return end === -1 ? text.length : end;
}

function clamp(text: string, from: number, to: number): [number, number] {
  const a = Math.max(0, Math.min(from, text.length));
  const b = Math.max(a, Math.min(to, text.length));
  return [a, b];
}
