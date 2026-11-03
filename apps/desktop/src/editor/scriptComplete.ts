import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import type { Diagnostic } from "@codemirror/lint";
import type { EditorState } from "@codemirror/state";

import type { VariableTable } from "../lib/variables";

/** Before the request is sent, or after its response arrives. */
export type ScriptPhase = "pre" | "post";

type Entry = Completion & { postOnly?: boolean };

const fn = (label: string, args: string, info: string, postOnly?: boolean): Entry => ({
  label,
  type: "method",
  detail: `(${args})`,
  info,
  postOnly,
});
const prop = (label: string, detail: string, info: string, postOnly?: boolean): Entry => ({
  label,
  type: "property",
  detail,
  info,
  postOnly,
});

const STORE: Entry[] = [
  fn("get", "name", "The variable's value, or undefined."),
  fn("set", "name, value", "Sets a variable for the steps after this one."),
  fn("has", "name", "Whether the variable has a value."),
  fn("unset", "name", "Removes the variable."),
  fn("toObject", "", "Every variable, as an object."),
  fn("replaceIn", "text", "The text with its {{variables}} filled in."),
  fn("clear", "", "Removes every variable."),
];

/**
 * What the script sandbox offers (crates/script/src/prelude.js), by the
 * path before the dot: Bruno's `bru`, `req`, `res`, Postman's `pm`, and the
 * globals both have.
 */
const MEMBERS: Record<string, Entry[]> = {
  "": [
    { label: "bru", type: "namespace", detail: "Bruno", info: "Variables, sendRequest, sleep and interpolate." },
    { label: "req", type: "namespace", detail: "the request", info: "Read or change the request before it is sent." },
    { label: "res", type: "namespace", detail: "the response", info: "Status, headers and body of the response.", postOnly: true } as Entry,
    { label: "pm", type: "namespace", detail: "Postman", info: "Postman's sandbox: variables, request, response, test." },
    fn("test", "name, fn", "A named check: passes unless fn throws."),
    fn("expect", "value", "Chai-style assertion: expect(x).to.equal(y)."),
    { label: "console", type: "namespace", detail: "log", info: "console.log(...) prints to the run's log." },
    { label: "Buffer", type: "class", detail: "Node's Buffer", info: "Buffer.from(text, 'base64').toString('utf8')" },
    fn("btoa", "text", "Base64-encodes Latin-1 text."),
    fn("atob", "base64", "Decodes base64 to Latin-1 text."),
  ],
  bru: [
    fn("getVar", "name", "A variable's value."),
    fn("setVar", "name, value", "Sets a variable for the steps after this one."),
    fn("hasVar", "name", "Whether the variable has a value."),
    fn("deleteVar", "name", "Removes the variable."),
    fn("getEnvVar", "name", "An environment variable's value."),
    fn("setEnvVar", "name, value", "Sets a variable (the flow has one store)."),
    fn("getCollectionVar", "name", "A variable's value."),
    fn("getRequestVar", "name", "A variable's value."),
    fn("getProcessEnv", "name", "Always undefined: scripts cannot read the machine's environment."),
    fn("interpolate", "text", "The text with its {{variables}} filled in."),
    fn("sendRequest", "{ url, method, headers, data }", "Sends a request; await it for { status, headers, data }."),
    fn("sleep", "ms", "Waits; use with await."),
  ],
  req: [
    fn("getUrl", "", "The URL, with variables filled in."),
    fn("setUrl", "url", "Changes the URL."),
    fn("getMethod", "", "GET, POST, ..."),
    fn("setMethod", "method", "Changes the method."),
    fn("getHeader", "name", "A header's value."),
    fn("getHeaders", "", "Every header, as an object."),
    fn("setHeader", "name, value", "Adds or replaces a header."),
    fn("deleteHeader", "name", "Removes a header."),
    fn("getBody", "{ raw }?", "The body, parsed as JSON when it is."),
    fn("setBody", "body", "Replaces the body; objects become JSON."),
    fn("getName", "", "The step's name."),
  ],
  res: [
    prop("status", "number", "The status code."),
    prop("statusText", "string", "The status text."),
    prop("body", "any", "The body, parsed as JSON when it is."),
    prop("headers", "object", "Headers by lower-case name."),
    prop("responseTime", "ms", "How long the request took."),
    fn("getStatus", "", "The status code."),
    fn("getStatusText", "", "The status text."),
    fn("getBody", "", "The body, parsed as JSON when it is."),
    fn("getHeader", "name", "A header's value."),
    fn("getHeaders", "", "Headers by lower-case name."),
    fn("getResponseTime", "", "How long the request took, in ms."),
  ],
  pm: [
    prop("variables", "store", "get / set / has / unset variables."),
    prop("environment", "store", "The same store as pm.variables."),
    prop("collectionVariables", "store", "The same store as pm.variables."),
    prop("globals", "store", "The same store as pm.variables."),
    prop("iterationData", "object", "Always empty: there is no data file."),
    prop("info", "object", "requestName, eventName, iteration."),
    fn("test", "name, fn", "A named check: passes unless fn throws."),
    fn("expect", "value", "Chai-style assertion."),
    prop("request", "object", "The request: method, url, headers, body."),
    prop("response", "object", "The response: code, json(), headers, to.", true),
    fn("sendRequest", "request, callback?", "Sends a request; await it or pass (err, res) => {}."),
  ],
  "pm.variables": STORE,
  "pm.environment": STORE,
  "pm.collectionVariables": STORE,
  "pm.globals": STORE,
  "pm.info": [
    prop("requestName", "string", "The step's name."),
    prop("eventName", "string", "prerequest or test."),
    prop("iteration", "number", "Always 0."),
    prop("iterationCount", "number", "Always 1."),
  ],
  "pm.request": [
    prop("method", "string", "GET, POST, ...; can be set."),
    prop("url", "object", "toString(), update(url), getHost()."),
    prop("headers", "object", "get, has, add, upsert, remove."),
    prop("body", "object", "raw, update(body)."),
  ],
  "pm.request.url": [
    fn("toString", "", "The URL."),
    fn("update", "url", "Changes the URL."),
    fn("getHost", "", "The host name."),
  ],
  "pm.request.headers": [
    fn("get", "name", "A header's value."),
    fn("has", "name", "Whether the header is there."),
    fn("add", "{ key, value }", "Adds a header."),
    fn("upsert", "{ key, value }", "Adds or replaces a header."),
    fn("remove", "name", "Removes a header."),
    fn("toObject", "", "Every header, as an object."),
    fn("each", "fn", "Calls fn({ key, value }) for each header."),
  ],
  "pm.request.body": [
    prop("raw", "string", "The body text."),
    fn("update", "body", "Replaces the body."),
    fn("toString", "", "The body text."),
  ],
  "pm.response": [
    prop("code", "number", "The status code."),
    prop("status", "string", "The status text."),
    prop("responseTime", "ms", "How long the request took."),
    prop("responseSize", "bytes", "The body's length."),
    fn("text", "", "The body as text."),
    fn("json", "", "The body parsed as JSON."),
    prop("headers", "object", "get(name), has(name), toObject()."),
    prop("to", "assertions", "pm.response.to.have.status(200)"),
  ],
  "pm.response.headers": [
    fn("get", "name", "A header's value."),
    fn("has", "name", "Whether the header is there."),
    fn("toObject", "", "Headers by lower-case name."),
  ],
  "pm.response.to": [
    prop("have", "", "status, header, body, jsonBody"),
    prop("be", "", "ok, success, json, notFound, ..."),
    prop("not", "", "Negates what follows."),
  ],
  "pm.response.to.have": [
    fn("status", "code | text", "Checks the status."),
    fn("header", "name, value?", "Checks a header is there, or has this value."),
    fn("body", "text?", "Checks the body contains the text."),
    fn("jsonBody", "path?", "Checks the body is JSON, with this path."),
  ],
  "pm.response.to.be": [
    "ok", "success", "info", "redirection", "clientError", "serverError", "error",
    "accepted", "badRequest", "unauthorized", "forbidden", "notFound", "json",
  ].map((w) => prop(w, "", `Checks the response is ${w}.`)),
  console: ["log", "info", "warn", "error", "debug"].map((w) => fn(w, "...values", "Prints to the run's log.")),
  Buffer: [
    fn("from", "value, encoding?", "utf8, base64, base64url, hex, latin1."),
    fn("isBuffer", "value", "Whether it is a Buffer."),
    fn("byteLength", "value, encoding?", "The length in bytes."),
    fn("concat", "list", "Joins Buffers."),
  ],
};
for (const [path, entries] of Object.entries(MEMBERS)) {
  if (path.startsWith("pm.response")) for (const e of entries) e.postOnly = true;
}

/** Words that chain after `expect(value).` */
const CHAIN: Completion[] = [
  ...["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "not", "deep"].map(
    (w) => ({ label: w, type: "keyword", boost: -1 }),
  ),
  ...["ok", "true", "false", "null", "undefined", "NaN", "exist", "empty"].map((w) => ({
    label: w,
    type: "property",
    info: `Checks the value is ${w}.`,
  })),
  ...[
    ["equal", "value"],
    ["eql", "value"],
    ["above", "n"],
    ["below", "n"],
    ["least", "n"],
    ["most", "n"],
    ["within", "low, high"],
    ["a", "type"],
    ["an", "type"],
    ["include", "value"],
    ["property", "name, value?"],
    ["lengthOf", "n"],
    ["match", "regexp"],
    ["keys", "...names"],
    ["oneOf", "list"],
    ["members", "list"],
    ["satisfy", "fn"],
  ].map(([label, args]) => ({ label, type: "method", detail: `(${args})` })),
];

const VAR_CALL =
  /\b(?:getVar|setVar|hasVar|deleteVar|getEnvVar|setEnvVar|getCollectionVar|getRequestVar|get|set|has|unset)\(\s*["'`]([^"'`]*)$/;

/**
 * Completion for scripts: members of `bru`, `req`, `res`, `pm` and their
 * parts after a dot, chai words after `expect(...)`, and variable names
 * inside `bru.getVar("…")`, `pm.variables.set("…")` and `{{…}}`. The
 * response is offered only to post-response scripts.
 */
export function scriptCompletions(phase: ScriptPhase, variables: () => VariableTable) {
  const allowed = (e: Entry) => phase === "post" || !e.postOnly;
  return (context: CompletionContext): CompletionResult | null => {
    const line = context.state.doc.lineAt(context.pos);
    const before = line.text.slice(0, context.pos - line.from);

    // A variable's name: in a get/set call, or between {{ }} in a string.
    const call = VAR_CALL.exec(before);
    const braces = /\{\{\s*([\w.$-]*)$/.exec(before);
    if (call || braces) {
      const typed = (call ?? braces)![1];
      const table = variables();
      return {
        from: context.pos - typed.length,
        options: [...table.entries()]
          .filter(([, info]) => info.kind !== "generated" || braces)
          .map(([name, info]) => ({
            label: name,
            apply: braces && !line.text.slice(context.pos - line.from).startsWith("}}") ? `${name}}}` : name,
            type: "variable",
            detail: info.value != null ? `= ${info.value.length > 30 ? `${info.value.slice(0, 29)}…` : info.value}` : info.kind,
            info: info.source,
          })),
        validFor: /^[\w.$-]*$/,
      };
    }

    const inside = syntaxTree(context.state).resolveInner(context.pos, -1).name;
    if (/String|Comment/.test(inside)) return null;

    // After `expect(...).` or `expect(...).to.be.`
    const chain = /\bexpect\((?:[^()]|\([^()]*\))*\)(?:\.\w+(?:\((?:[^()]|\([^()]*\))*\))?)*\.(\w*)$/.exec(before);
    if (chain) return { from: context.pos - chain[1].length, options: CHAIN, validFor: /^\w*$/ };

    // A member: `bru.` or `pm.response.to.`
    const member = /(?:^|[^\w$.])([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\.([\w$]*)$/.exec(before);
    if (member) {
      const entries = MEMBERS[member[1]];
      if (!entries) return null;
      if (member[1] === "res" && phase === "pre") return null;
      return { from: context.pos - member[2].length, options: entries.filter(allowed), validFor: /^[\w$]*$/ };
    }

    // A global, as a word starts or when asked for with Ctrl+Space.
    const word = /(?:^|[^\w$.])([A-Za-z_$][\w$]*)$/.exec(before);
    if (!word && !context.explicit) return null;
    const typed = word?.[1] ?? "";
    return { from: context.pos - typed.length, options: MEMBERS[""].filter(allowed), validFor: /^[\w$]*$/ };
  };
}

/** Syntax errors, from the parser's error nodes; scripts run as async functions, so `await` is fine. */
export function scriptSyntaxErrors(state: EditorState): Diagnostic[] {
  const tree = ensureSyntaxTree(state, state.doc.length, 500) ?? syntaxTree(state);
  const out: Diagnostic[] = [];
  const length = state.doc.length;
  tree.iterate({
    enter: (node) => {
      if (!node.type.isError) return;
      let from = node.from;
      let to = Math.max(node.to, from + 1);
      if (to > length) {
        to = length;
        from = Math.max(0, length - 1);
      }
      // One mark per spot is enough.
      if (out.some((d) => d.from === from)) return;
      out.push({
        from,
        to,
        severity: "error",
        message: from >= length - 1 ? "The script ends too early: something is not closed" : "Syntax error",
      });
    },
  });
  return out;
}
