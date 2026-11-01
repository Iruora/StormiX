import { createContext, useContext } from "react";

import type { FlowResult, Step, TestFile } from "../bindings";

/** What a `{{name}}` stands for, as far as the app can tell before sending. */
export type VariableInfo = {
  /** Null when only a run can tell, like a value a step extracts. */
  value: string | null;
  /** Where the value comes from, like `flow variables` or `extracted by "Log in"`. */
  source: string;
  kind: "defined" | "runtime" | "generated";
};

export type VariableTable = Map<string, VariableInfo>;

const GENERATED: Record<string, string> = {
  $guid: "a random UUID v4",
  $randomUUID: "a random UUID v4",
  $timestamp: "Unix time in seconds",
  $timestampMs: "Unix time in milliseconds",
  $randomInt: "an integer from 0 to 1000",
};

/**
 * Every variable the file can use, later sources winning as in a run: the
 * flow's own, then the chosen environment, then what steps extract (with
 * the value from the last run, when there is one).
 */
export function variableTable(
  test: TestFile | null | undefined,
  environment: Record<string, string>,
  envName: string,
  lastRun?: FlowResult | null,
): VariableTable {
  const table: VariableTable = new Map();
  for (const [name, description] of Object.entries(GENERATED)) {
    table.set(name, { value: null, source: `generated on each use: ${description}`, kind: "generated" });
  }
  for (const [name, value] of Object.entries(test?.flow.variables ?? {})) {
    table.set(name, { value, source: "flow variables", kind: "defined" });
  }
  for (const [name, value] of Object.entries(environment)) {
    table.set(name, { value, source: `environment "${envName}"`, kind: "defined" });
  }
  // Disabled steps are not sent, so they set nothing.
  for (const step of (test?.flow.steps ?? []).filter((s) => s.enabled !== false)) {
    const scripts = `${step.scripts?.pre_request ?? ""}\n${step.scripts?.test ?? ""}`;
    for (const m of scripts.matchAll(SCRIPT_SET)) {
      const ran = lastRun?.steps.find((s) => s.name === step.name)?.extracted[m[1]];
      if (table.get(m[1])?.kind === "defined" && ran == null) continue;
      table.set(m[1], {
        value: ran ?? null,
        source: ran != null ? `set by a script in "${step.name}" in the last run` : `set by a script in "${step.name}" when it runs`,
        kind: "runtime",
      });
    }
    for (const name of Object.keys(step.extract ?? {})) {
      const ran = lastRun?.steps.find((s) => s.name === step.name)?.extracted[name];
      table.set(name, {
        value: ran ?? null,
        source: ran != null ? `extracted by "${step.name}" in the last run` : `extracted by "${step.name}" when it runs`,
        kind: "runtime",
      });
    }
  }
  return table;
}

/** Names a step sets for the steps after it: its extracts and what its scripts set. */
export function variablesSetBy(step: Step): string[] {
  const scripts = `${step.scripts?.pre_request ?? ""}\n${step.scripts?.test ?? ""}`;
  return [...Object.keys(step.extract ?? {}), ...[...scripts.matchAll(SCRIPT_SET)].map((m) => m[1])];
}

/** Names a step's request uses as `{{name}}`: URL, headers and body. */
export function variablesUsedBy(step: Step): string[] {
  const { url, headers, body } = step.request;
  const texts = [url, ...(headers ?? []).map((h) => h.value)];
  if (body) texts.push(JSON.stringify(body));
  const names = new Set<string>();
  for (const text of texts) for (const m of text.matchAll(VARIABLE)) names.add(m[1]);
  return [...names];
}

export const VariablesContext = createContext<VariableTable>(new Map());

export function useVariables(): VariableTable {
  return useContext(VariablesContext);
}

/** `pm.variables.set("x", …)`, `bru.setVar("x", …)` and their kin. */
const SCRIPT_SET = /\b(?:set|setVar|setEnvVar|setGlobalVar)\(\s*["'`]([^"'`]+)["'`]\s*,/g;

/** `{{ name }}` occurrences, with their name and where they are. */
export const VARIABLE = /\{\{\s*([^{}\s]+)\s*\}\}/g;

/** The text a hover shows for one variable. */
export function describeVariable(name: string, info: VariableInfo | undefined): string {
  if (!info) return `{{${name}}} is not defined: it is sent as written unless an environment or a script sets it.`;
  if (info.kind === "generated") return `{{${name}}}: ${info.source}`;
  if (info.value == null) return `{{${name}}}: ${info.source}`;
  return `{{${name}}} = ${info.value}\n${info.source}`;
}
