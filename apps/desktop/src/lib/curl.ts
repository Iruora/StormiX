import { createContext, useContext } from "react";

import type { CurlCommand, FlowResult, Shell, Step } from "../bindings";

/**
 * Builds a step's request as a `curl` command with today's variables: the
 * chosen environment with its session changes and secrets, and what earlier
 * steps extracted in the last run.
 */
export type CurlMaker = {
  make: (step: Step, shell: Shell) => Promise<CurlCommand>;
  /** The environment's secret names, to say when a command holds one. */
  secrets: string[];
};

export const CurlContext = createContext<CurlMaker | null>(null);

export function useCurlMaker(): CurlMaker | null {
  return useContext(CurlContext);
}

/** The shell a copied command is quoted for, unless the user picks the other. */
export const defaultShell = (): Shell =>
  typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent) ? "cmd" : "posix";

export const shellName = (shell: Shell) => (shell === "cmd" ? "Windows cmd" : "bash");

/**
 * What the steps before `name` extracted in the last run, later values
 * winning, as a run would have them when it reached that step. A step the
 * run did not have gets every value the run extracted.
 */
export function extractedBefore(run: FlowResult | null, name: string): Record<string, string> {
  const values: Record<string, string> = {};
  for (const step of run?.steps ?? []) {
    if (step.name === name) break;
    Object.assign(values, step.extracted);
  }
  return values;
}
