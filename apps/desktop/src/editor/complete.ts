import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { EditorState } from "@codemirror/state";
import { parse } from "yaml";

/**
 * Suggests the keys the test file format allows where the cursor is, the
 * allowed values of a few keys, and variable names inside `{{ }}`.
 * The position is worked out from indentation, which is enough for the
 * block style the app and `stormix init` write.
 */

type Key = { label: string; info: string; value?: string };

const STEP_KEYS: Key[] = [
  { label: "name", info: "Shown in reports; unique names make comparisons clearer" },
  { label: "enabled", info: "false keeps the step in place but skips it in runs", value: "false" },
  { label: "cache", info: "true reuses the step's response instead of sending it again, until cleared", value: "true" },
  { label: "request", info: "method, url, headers and body" },
  { label: "extract", info: "Variables to set from the response" },
  { label: "assert", info: "Checks on the response" },
  { label: "scripts", info: "pre_request and test JavaScript" },
];

const KEYS: Record<string, Key[]> = {
  "": [
    { label: "version", info: "Always 1 for now", value: "1" },
    { label: "flow", info: "The requests, run in order" },
    { label: "load", info: "A load profile: constant_rate, ramping or virtual_users" },
    { label: "thresholds", info: "Pass/fail rules under load, like `p95 < 300ms`" },
  ],
  flow: [
    { label: "name", info: "Shown in reports and in the run history" },
    { label: "description", info: "Free text" },
    { label: "variables", info: "Names to string values, used as {{name}}" },
    { label: "steps", info: "One or more steps, run in order" },
    { label: "set_aside", info: "Steps kept out of the flow: never run, ready to put back" },
    {
      label: "trace",
      info: "Send a W3C traceparent header for Dynatrace and other APM tools: request (a trace per request) or flow (a trace per run)",
      value: "request",
    },
  ],
  "flow/steps/[]": STEP_KEYS,
  "flow/steps/[]/request": [
    { label: "method", info: "GET, POST, PUT, PATCH, DELETE, HEAD or OPTIONS" },
    { label: "url", info: "The full URL; query strings go here too" },
    { label: "headers", info: "A list of { name, value }" },
    { label: "body", info: "json, raw, form, multipart or graphql" },
  ],
  "flow/steps/[]/request/headers/[]": [
    { label: "name", info: "Header name" },
    { label: "value", info: "Header value; {{variables}} work" },
  ],
  "flow/steps/[]/request/body": [
    { label: "json", info: "Any YAML value, sent as application/json" },
    { label: "raw", info: "Text sent as written" },
    { label: "form", info: "[{ key, value }] as x-www-form-urlencoded" },
    { label: "multipart", info: "[{ key, value }] as multipart/form-data" },
    { label: "graphql", info: "{ query, variables }: a GraphQL operation; errors in the response fail the step" },
  ],
  "flow/steps/[]/request/body/graphql": [
    { label: "query", info: "The GraphQL document; use | for several lines", value: "|" },
    { label: "variables", info: "An object; {{name}} values take the type the query declares, like $id: Int!" },
    { label: "operation_name", info: "Which operation to run when the query holds several" },
    { label: "allow_errors", info: "true passes the step even when the response has GraphQL errors", value: "true" },
  ],
  "flow/steps/[]/request/body/form/[]": [
    { label: "key", info: "Field name" },
    { label: "value", info: "Field value" },
  ],
  "flow/steps/[]/request/body/multipart/[]": [
    { label: "key", info: "Field name" },
    { label: "value", info: "Field value" },
  ],
  "flow/steps/[]/extract/*": [
    { label: "json", info: "A JSONPath into the body, like $.data.token" },
    { label: "header", info: "A response header, matched case-insensitively" },
    { label: "regex", info: "The first capture group, else the whole match" },
  ],
  "flow/steps/[]/assert/[]": [
    { label: "status", info: "The status code is exactly this", value: "200" },
    { label: "max_duration_ms", info: "The full response arrived within this many ms", value: "500" },
    { label: "header", info: "{ name, equals | contains }" },
    { label: "json", info: "{ path, equals | exists }" },
    { label: "body_contains", info: "The body contains this text" },
  ],
  "flow/steps/[]/assert/[]/header": [
    { label: "name", info: "Header name; alone, checks it is present" },
    { label: "equals", info: "The value exactly" },
    { label: "contains", info: "Text the value contains" },
  ],
  "flow/steps/[]/assert/[]/json": [
    { label: "path", info: "A JSONPath; alone, checks it matches something" },
    { label: "equals", info: "Any YAML value" },
    { label: "exists", info: "false: the path matches nothing", value: "false" },
  ],
  "flow/steps/[]/scripts": [
    { label: "pre_request", info: "JavaScript run before the request", value: "|" },
    { label: "test", info: "JavaScript run on the response; pm.test adds checks", value: "|" },
  ],
  load: [
    { label: "constant_rate", info: "{ rate, duration }: rate iterations start each second" },
    { label: "ramping", info: "{ start_rate, stages }: move linearly between rates" },
    { label: "virtual_users", info: "{ count, duration, think_time }: users loop the flow" },
  ],
  "load/constant_rate": [
    { label: "rate", info: "Iterations started each second" },
    { label: "duration", info: "Like 30s, 5m" },
  ],
  "load/ramping": [
    { label: "start_rate", info: "Rate at the start (default 0)" },
    { label: "stages", info: "[{ duration, target }]" },
  ],
  "load/ramping/stages/[]": [
    { label: "duration", info: "Like 30s, 2m" },
    { label: "target", info: "Rate reached at the end of the stage" },
  ],
  "load/virtual_users": [
    { label: "count", info: "Number of users" },
    { label: "duration", info: "Like 10m" },
    { label: "think_time", info: "Pause between loops, like 1s" },
  ],
};

const VALUES: Record<string, string[]> = {
  "flow/steps/[]/request/method": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
  "flow/steps/[]/assert/[]/json/exists": ["true", "false"],
  "flow/steps/[]/enabled": ["true", "false"],
  "flow/steps/[]/cache": ["true", "false"],
  "flow/trace": ["request", "flow"],
  "flow/steps/[]/request/body/graphql/allow_errors": ["true", "false"],
  version: ["1"],
};

const THRESHOLDS = ["p95 < 300ms", "p99 < 1s", "avg < 100ms", "errors < 1%", "checks > 99%", "rps > 100"];

const DYNAMIC = ["$guid", "$randomUUID", "$timestamp", "$timestampMs", "$randomInt"];

/** Keys of the lines that hold the cursor's line, outermost first; `[]` marks a list item. */
export function pathAt(state: EditorState, pos: number): { path: string[]; line: string } {
  const line = state.doc.lineAt(pos);
  const before = line.text.slice(0, pos - line.from);
  const own = /^(\s*)(-\s+)?/.exec(before)!;
  const path: string[] = [];
  let limit = own[1].length + (own[2]?.length ?? 0);
  if (own[2]) {
    path.push("[]");
    limit = own[1].length;
  }
  for (let n = line.number - 1; n >= 1 && limit > 0; n--) {
    const text = state.doc.line(n).text;
    const m = /^(\s*)(-\s+)?([^\s:#][^:#]*?)?\s*:(\s|$)/.exec(text);
    const blank = /^\s*(#.*)?$/.test(text);
    if (blank) continue;
    const dash = m ? m[1].length : /^\s*/.exec(text)![0].length;
    const content = m ? dash + (m[2]?.length ?? 0) : dash;
    if (m?.[3] && content < limit) {
      path.push(m[3].trim());
      if (m[2]) {
        path.push("[]");
        limit = dash;
      } else {
        limit = content;
      }
    } else if ((m?.[2] || /^\s*-\s/.test(text)) && dash < limit) {
      path.push("[]");
      limit = dash;
    } else if (!m && content < limit) {
      // A plain list entry or scalar continuation; its parent is further up.
      limit = content;
    }
  }
  return { path: path.reverse(), line: before };
}

function keysFor(path: string[]): Key[] | undefined {
  // Set-aside steps are steps like any other.
  if (path[0] === "flow" && path[1] === "set_aside") path = ["flow", "steps", ...path.slice(2)];
  const exact = KEYS[path.join("/")];
  if (exact) return exact;
  // Extract entries are named by the user: `extract/<name>`.
  if (path.length >= 2 && path[path.length - 2] === "extract") {
    return KEYS[[...path.slice(0, -1), "*"].join("/")];
  }
  return undefined;
}

export function testFileCompletions(extraVariables: () => string[]) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const { state, pos } = ctx;
    const lineStart = state.doc.lineAt(pos).from;
    const before = state.sliceDoc(lineStart, pos);

    const variable = /\{\{([\w$.-]*)$/.exec(before);
    if (variable) {
      const after = state.sliceDoc(pos, pos + 2);
      const names = new Set([...variablesIn(state.doc.toString()), ...extraVariables(), ...DYNAMIC]);
      return {
        from: pos - variable[1].length,
        options: [...names].map((name) => ({
          label: name,
          type: name.startsWith("$") ? "function" : "variable",
          apply: after === "}}" ? name : `${name}}}`,
        })),
        validFor: /^[\w$.-]*$/,
      };
    }

    const { path } = pathAt(state, pos);
    const value = /^(\s*)(-\s+)?([\w$]+):\s+([\w$]*)$/.exec(before);
    if (value) {
      const key = [...path, value[3]].join("/").replace(/^flow\/set_aside\//, "flow/steps/");
      const values = VALUES[key];
      if (!values) return null;
      return {
        from: pos - value[4].length,
        options: values.map((v) => ({ label: v, type: "constant" })),
        validFor: /^[\w$]*$/,
      };
    }

    const key = /^(\s*)(-\s+)?([\w$]*)$/.exec(before);
    if (!key) return null;
    if (!ctx.explicit && key[3].length === 0 && !key[2]) return null;

    if (path.join("/") === "thresholds/[]" && key[2]) {
      return {
        from: pos - key[3].length,
        options: THRESHOLDS.map((t) => ({ label: t, type: "text" })),
      };
    }
    const keys = keysFor(path);
    if (!keys) return null;
    const used = siblingKeys(state, pos, key[1].length + (key[2]?.length ?? 0), !!key[2]);
    const options: Completion[] = keys
      .filter((k) => !used.has(k.label))
      .map((k) => ({
        label: k.label,
        type: "property",
        info: k.info,
        apply: `${k.label}: ${k.value ?? ""}`,
      }));
    return { from: pos - key[3].length, options, validFor: /^[\w$]*$/ };
  };
}

/** Keys already written at this level, so they are not offered twice. */
function siblingKeys(state: EditorState, pos: number, indent: number, newItem: boolean): Set<string> {
  const used = new Set<string>();
  if (newItem) return used;
  const here = state.doc.lineAt(pos).number;
  const keyAt = (text: string) => {
    const m = /^(\s*)(-\s+)?([\w$]+)\s*:/.exec(text);
    if (!m) return null;
    return { indent: m[1].length + (m[2]?.length ?? 0), key: m[3], item: !!m[2] };
  };
  for (const dir of [-1, 1]) {
    for (let n = here + dir; n >= 1 && n <= state.doc.lines; n += dir) {
      const text = state.doc.line(n).text;
      if (/^\s*(#.*)?$/.test(text)) continue;
      const k = keyAt(text);
      if (k && k.indent === indent) {
        // A `- key:` line starts a list item: ours going up, the next one going down.
        if (k.item && dir === 1) break;
        used.add(k.key);
        if (k.item) break;
        continue;
      }
      if (/^\s*/.exec(text)![0].length < indent) break;
    }
  }
  return used;
}

/** Names the file defines: flow variables and extracted ones. */
function variablesIn(text: string): string[] {
  try {
    const doc = parse(text) as { flow?: { variables?: Record<string, unknown>; steps?: { extract?: Record<string, unknown> }[] } };
    const names = Object.keys(doc?.flow?.variables ?? {});
    for (const step of doc?.flow?.steps ?? []) names.push(...Object.keys(step?.extract ?? {}));
    return names;
  } catch {
    // Mid-edit text often does not parse; fall back to what it mentions.
    return [...new Set([...text.matchAll(/\{\{([\w.-]+)\}\}/g)].map((m) => m[1]))];
  }
}
