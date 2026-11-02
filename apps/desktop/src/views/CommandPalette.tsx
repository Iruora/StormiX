import { Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

/** Something the palette can do. */
export type Command = {
  id: string;
  label: string;
  /** The heading it shows under, like "File" or "Go to step". */
  group: string;
  /** Shown at the end, like a keyboard shortcut. */
  hint?: string;
  /** Other words it is found by. */
  keywords?: string;
  disabled?: boolean;
  run: () => void;
};

/** True when every word typed starts a word of the command, or appears in it. */
export function matches(command: Command, query: string): boolean {
  const text = `${command.group} ${command.label} ${command.keywords ?? ""}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((word) => text.includes(word));
}

/**
 * Finds any action by typing a few letters of it, like "import bruno" or
 * "no proxy". Opened with Ctrl K (⌘ K on macOS).
 */
export function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const found = useMemo(() => commands.filter((c) => !c.disabled && matches(c, query)), [commands, query]);
  useEffect(() => setAt(0), [query]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${at}"]`)?.scrollIntoView({ block: "nearest" });
  }, [at]);

  const pick = (c: Command | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };

  let lastGroup = "";
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-ink/20 px-4 pt-[12vh]"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-label="Search actions"
        className="flex max-h-[70vh] w-[34rem] max-w-full flex-col overflow-hidden rounded-xl border border-line bg-surface shadow-2xl"
      >
        <label className="flex items-center gap-2 border-b border-line px-3 py-2.5">
          <Search size="0.9375rem" className="shrink-0 text-muted" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setAt((i) => Math.min(i + 1, found.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setAt((i) => Math.max(i - 1, 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                pick(found[at]);
              } else if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              }
            }}
            placeholder="Search actions, environments and steps"
            aria-label="Search actions"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={found[at] ? `palette-${found[at].id}` : undefined}
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-subtle"
          />
          <kbd className="rounded border border-line px-1.5 text-[0.6875rem] text-subtle">Esc</kbd>
        </label>
        <ul id="palette-list" ref={listRef} role="listbox" className="min-h-0 flex-1 overflow-y-auto p-1">
          {found.length === 0 && <li className="px-3 py-4 text-center text-xs text-muted">Nothing matches “{query}”.</li>}
          {found.map((c, i) => {
            const heading = c.group !== lastGroup ? c.group : null;
            lastGroup = c.group;
            return (
              <li key={c.id} role="presentation">
                {heading && (
                  <div className="px-2 pb-0.5 pt-2 text-[0.625rem] font-semibold uppercase tracking-wider text-subtle">
                    {heading}
                  </div>
                )}
                <div
                  id={`palette-${c.id}`}
                  role="option"
                  aria-selected={i === at}
                  data-index={i}
                  onMouseMove={() => setAt(i)}
                  onClick={() => pick(c)}
                  className={`flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-xs ${
                    i === at ? "bg-accent-soft text-accent" : "text-ink"
                  }`}
                >
                  <span className="min-w-0 flex-1 truncate">{c.label}</span>
                  {c.hint && <span className="shrink-0 text-[0.6875rem] text-subtle">{c.hint}</span>}
                </div>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
