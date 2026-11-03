import { createContext, useContext } from "react";

import type { EnvOverrides } from "../views/EnvEditor";

/**
 * What a variable's hover needs to change a value in place, with the same
 * rules as the Variables dialog: an environment value changes for this
 * session first and reaches its file only when saved; a secret stays in
 * memory; a flow variable is part of the test file itself.
 */
export type VariableEditing = {
  /** The chosen environment, or null when none is. */
  envName: string | null;
  /** What the environment's file holds, to tell a session change from it. */
  fileValues: Record<string, string>;
  secrets: string[];
  overrides: EnvOverrides;
  /** Where saving writes, or null when it will ask. */
  savesTo: string | null;
  /** Changes an environment value for this session only. */
  setEnvVar: (name: string, value: string) => void;
  /** Goes back to the file's value. */
  resetEnvVar: (name: string) => void;
  /** Writes this value to the environment's file. */
  saveEnvVar: (name: string, value: string) => Promise<void>;
  setSecret: (name: string, value: string) => void;
  /** Changes, or adds, one of the flow's own variables in the test file. */
  setFlowVar: (name: string, value: string) => void;
};

export const VariableEditContext = createContext<VariableEditing | null>(null);

export function useVariableEditing(): VariableEditing | null {
  return useContext(VariableEditContext);
}
