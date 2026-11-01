import { Check, FileCode, FolderOpen, Layers, Pencil, Plus, Trash2, X } from "lucide-react";
import { useState, type ReactNode } from "react";

import type { Workspace_Serialize as Workspace } from "../bindings";
import { savedAgo } from "../lib/workspace";
import { Spinner } from "./Busy";

/**
 * Every saved workspace: what was open, with which environment. The open
 * one is saved as you work; the last one used comes back on launch.
 */
export function WorkspacesView({
  items,
  activeId,
  loading,
  onOpen,
  onNew,
  onRename,
  onDelete,
}: {
  items: Workspace[];
  activeId: string | null;
  loading: boolean;
  onOpen: (ws: Workspace) => void;
  onNew: () => void;
  onRename: (ws: Workspace, name: string) => void;
  onDelete: (ws: Workspace) => void;
}) {
  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-surface px-5 py-3">
        <h1 className="text-[0.9375rem] font-semibold">Workspaces</h1>
        <span className="min-w-0 flex-1 basis-48 text-xs text-muted">
          What you had open is saved as you work, and the last workspace opens again when the app starts.
        </span>
        <button
          type="button"
          onClick={onNew}
          className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-white hover:opacity-90"
        >
          <Plus size="0.8125rem" />
          New workspace
        </button>
      </header>
      {items.length === 0 ? (
        <div className="grid flex-1 place-items-center text-center text-muted">
          <div>
            <Layers size="1.75rem" className="mx-auto mb-2 text-subtle" />
            {loading ? (
              <span className="inline-flex items-center gap-2">
                <Spinner size="0.875rem" className="text-accent" />
                Loading workspaces…
              </span>
            ) : (
              "No workspaces yet."
            )}
          </div>
        </div>
      ) : (
        <ul className="min-h-0 flex-1 space-y-2 overflow-y-auto p-4">
          {items.map((ws) => (
            <WorkspaceRow
              key={ws.id}
              ws={ws}
              active={ws.id === activeId}
              onOpen={() => onOpen(ws)}
              onRename={(name) => onRename(ws, name)}
              onDelete={() => onDelete(ws)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function WorkspaceRow({
  ws,
  active,
  onOpen,
  onRename,
  onDelete,
}: {
  ws: Workspace;
  active: boolean;
  onOpen: () => void;
  onRename: (name: string) => void;
  onDelete: () => void;
}) {
  const [naming, setNaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const envCount = Object.keys(ws.environments).length;
  const finishRename = () => {
    const name = naming?.trim();
    if (name && name !== ws.name) onRename(name);
    setNaming(null);
  };

  return (
    <li
      className={`flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border bg-surface px-4 py-3 ${
        active ? "border-accent" : "border-line"
      }`}
    >
      <FileCode size="1.125rem" className={active ? "text-accent" : "text-subtle"} />
      <div className="min-w-0 flex-1 basis-56">
        {naming !== null ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              finishRename();
            }}
            className="flex items-center gap-1"
          >
            <input
              autoFocus
              aria-label="Workspace name"
              value={naming}
              onChange={(e) => setNaming(e.target.value)}
              onBlur={finishRename}
              onKeyDown={(e) => e.key === "Escape" && setNaming(null)}
              className="min-w-0 flex-1 rounded border border-line bg-surface-2 px-1.5 py-0.5 text-[0.875rem] font-semibold"
            />
          </form>
        ) : (
          <div className="flex items-center gap-2">
            <span className="truncate font-semibold">{ws.name}</span>
            {active && (
              <span className="rounded bg-accent-soft px-1.5 py-0.5 text-[0.6875rem] font-medium text-accent">Open</span>
            )}
          </div>
        )}
        <div className="mono truncate text-xs text-muted" title={ws.path ?? undefined}>
          {ws.path ?? (ws.imported ? "Imported collection, not saved to a file" : ws.text ? "Not saved to a file" : "Example")}
          {ws.dirty && <span className="text-warn"> ● unsaved changes</span>}
        </div>
        <div className="mt-0.5 text-xs text-subtle">
          {ws.environment ? `Environment: ${ws.environment}` : envCount > 0 ? "No environment chosen" : "No environments"}
          {envCount > 1 && ` (${envCount} in the list)`} · saved {savedAgo(ws.updatedMs)}
        </div>
      </div>
      <div className="flex items-center gap-1">
        {!active && (
          <button
            type="button"
            onClick={onOpen}
            className="flex items-center gap-1.5 rounded-md border border-accent px-2.5 py-1 text-xs font-semibold text-accent hover:bg-accent-soft"
          >
            <FolderOpen size="0.8125rem" />
            Open
          </button>
        )}
        <IconButton label={`Rename ${ws.name}`} onClick={() => setNaming(ws.name)}>
          <Pencil size="0.8125rem" />
        </IconButton>
        {confirming ? (
          <span className="flex items-center gap-1 rounded-md bg-bad-soft px-1.5 py-0.5 text-xs text-bad">
            Delete?
            <IconButton label="Yes, delete it" onClick={onDelete}>
              <Check size="0.8125rem" />
            </IconButton>
            <IconButton label="No, keep it" onClick={() => setConfirming(false)}>
              <X size="0.8125rem" />
            </IconButton>
          </span>
        ) : (
          <IconButton label={`Delete ${ws.name}`} onClick={() => setConfirming(true)}>
            <Trash2 size="0.8125rem" />
          </IconButton>
        )}
      </div>
    </li>
  );
}

function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="grid size-6 place-items-center rounded text-muted hover:bg-surface-2 hover:text-ink"
    >
      {children}
    </button>
  );
}
