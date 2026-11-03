import { ChevronDown } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * A button that opens a list of actions below it. Closes on a pick, a
 * click outside or Escape; the arrow keys move between the actions.
 */
export function Menu({
  label,
  icon,
  title,
  align = "left",
  tone = "muted",
  width = "w-64",
  children,
}: {
  label: ReactNode;
  icon?: ReactNode;
  title?: string;
  align?: "left" | "right";
  tone?: "muted" | "warn";
  width?: string;
  /** The menu's content, given a function that closes it. */
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        button.current?.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const items = [...(list.current?.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])") ?? [])];
      if (!items.length) return;
      e.preventDefault();
      const at = items.indexOf(document.activeElement as HTMLElement);
      const next = e.key === "ArrowDown" ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
      items[next].focus();
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", keys);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", keys);
    };
  }, [open]);

  return (
    <div ref={box} className="relative">
      <button
        ref={button}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        title={title}
        onClick={() => setOpen(!open)}
        className={`flex max-w-full items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs hover:bg-surface-2 aria-expanded:bg-surface-2 ${
          tone === "warn" ? "text-warn" : "text-muted hover:text-ink"
        }`}
      >
        {icon}
        {label}
        <ChevronDown size="0.75rem" className="shrink-0" />
      </button>
      {open && (
        <div
          ref={list}
          role="menu"
          className={`absolute top-full z-40 mt-1 ${width} max-w-[calc(100vw-2rem)] whitespace-normal rounded-lg border border-line bg-surface p-1 shadow-lg ${
            align === "right" ? "right-0" : "left-0"
          }`}
        >
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

/** One action of a menu. */
export function MenuItem({
  children,
  onClick,
  icon,
  hint,
  disabled,
  checked,
  tone,
}: {
  children: ReactNode;
  onClick: () => void;
  icon?: ReactNode;
  /** Shown at the end, like a keyboard shortcut or a count. */
  hint?: ReactNode;
  disabled?: boolean;
  /** For a choice among several: whether this one is picked. */
  checked?: boolean;
  tone?: "bad";
}) {
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      disabled={disabled}
      onClick={onClick}
      className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs outline-none hover:bg-surface-2 focus-visible:bg-surface-2 disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent ${
        checked ? "bg-accent-soft text-accent hover:bg-accent-soft" : tone === "bad" ? "text-bad" : "text-ink"
      }`}
    >
      {icon !== undefined && <span className="grid w-4 shrink-0 place-items-center text-muted">{icon}</span>}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {hint && <span className="shrink-0 text-[0.6875rem] text-subtle">{hint}</span>}
    </button>
  );
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-2 pb-0.5 pt-2 text-[0.625rem] font-semibold uppercase tracking-wider text-subtle">{children}</div>
  );
}

export function MenuSeparator() {
  return <div role="separator" className="my-1 border-t border-line" />;
}

/** "Ctrl" or "⌘", as the keyboard shows it. */
export const MOD = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl";

/** A shortcut as shown beside an action, like "Ctrl ⇧ S". */
export function shortcut(key: string, shift = false) {
  return [MOD, shift ? "⇧" : null, key].filter(Boolean).join(" ");
}
