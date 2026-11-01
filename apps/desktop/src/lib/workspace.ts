import type { Workspace_Serialize as Workspace } from "../bindings";

export const WORKSPACES_KEY = ["workspaces"];

/** A workspace with nothing open yet: the example, no environment. */
export function newWorkspace(): Workspace {
  return {
    id: crypto.randomUUID(),
    name: "Untitled",
    renamed: false,
    updatedMs: 0,
    path: null,
    text: null,
    dirty: false,
    imported: false,
    environments: {},
    environment: "",
    tab: "steps",
    excluded: [],
    view: "file",
  };
}

/** The name a workspace goes by until it is renamed: its file's, else its flow's. */
export function workspaceTitle(path: string | null, flowName: string | null | undefined): string {
  const file = path?.split(/[\\/]/).pop()?.replace(/\.(stormix\.)?ya?ml$/i, "");
  return file || flowName?.trim() || "Untitled";
}

/** "just now", "5 min ago", "3 h ago", else the date. */
export function savedAgo(ms: number | null, now = Date.now()): string {
  if (!ms) return "not saved yet";
  const s = Math.max(0, (now - ms) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
