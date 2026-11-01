import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

// Sizes in rem, so they follow the text size the user has chosen.
/** The narrowest the panel gets. */
const MIN_REM = 18;
/** Room always kept for the editor beside the panel. */
const MAIN_MIN_REM = 26;
/** The width a new or reset panel opens at. */
const DEFAULT_REM = 28;
const STEP_REM = 1.5;

/** One rem in CSS pixels: 16 unless the system or webview text size is changed. */
function remPx(): number {
  const n = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(n) && n > 0 ? n : 16;
}

/** Whether the right panel is open and how wide it is, remembered between launches. */
export function usePanel() {
  const [open, setOpenState] = useState(() => stored("stormix.panel.open") !== "false");
  const [width, setWidthState] = useState(() => {
    // The width in rem; older versions stored pixels under another key.
    const rem = Number(stored("stormix.panel.rem"));
    if (Number.isFinite(rem) && rem > 0) return rem;
    const px = Number(stored("stormix.panel.width"));
    return Number.isFinite(px) && px > 0 ? px / 16 : DEFAULT_REM;
  });
  const setOpen = useCallback((next: boolean) => {
    setOpenState(next);
    store("stormix.panel.open", String(next));
  }, []);
  const setWidth = useCallback((next: number) => {
    setWidthState(next);
    store("stormix.panel.rem", next.toFixed(2));
  }, []);
  /** `width` and `setWidth` are in rem. */
  return { open, setOpen, width, setWidth };
}

/**
 * The main content with a panel on its right that can be closed and
 * resized by dragging its left edge. Widths are in rem, so a larger
 * system text size widens the panel along with its text. The panel stays
 * at least MIN_REM wide and leaves the editor at least MAIN_MIN_REM; when
 * the window is too small for both, they share it.
 */
export function SplitView({
  main,
  panel,
  open,
  width,
  onWidth,
}: {
  main: ReactNode;
  panel: ReactNode;
  open: boolean;
  width: number;
  onWidth: (width: number) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  // The space to share, in rem. It changes with the window and with the text size.
  const [room, setRoom] = useState(Infinity);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => setRoom(el.clientWidth / remPx());
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);
  // Too little room for both minimums: the panel takes 40%, never under 12rem.
  const min = room < MIN_REM + MAIN_MIN_REM ? Math.min(MIN_REM, Math.max(12, room * 0.4)) : MIN_REM;
  const max = Math.max(min, room - MAIN_MIN_REM);
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const shown = clamp(width);

  const startDrag = (e: React.PointerEvent) => {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    setDragging(true);
    const move = (ev: PointerEvent) => {
      const right = box.current?.getBoundingClientRect().right ?? 0;
      onWidth(clamp((right - ev.clientX) / remPx()));
    };
    const stop = () => {
      setDragging(false);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") onWidth(clamp(shown + STEP_REM));
    else if (e.key === "ArrowRight") onWidth(clamp(shown - STEP_REM));
    else return;
    e.preventDefault();
  };

  return (
    <div ref={box} className={`flex min-h-0 flex-1 ${dragging ? "cursor-col-resize select-none" : ""}`}>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">{main}</div>
      {open && (
        <>
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize the panel"
            aria-valuenow={Math.round(shown * remPx())}
            tabIndex={0}
            onPointerDown={startDrag}
            onDoubleClick={() => onWidth(clamp(DEFAULT_REM))}
            onKeyDown={onKey}
            title="Drag to resize, double-click to reset"
            className={`group relative w-px shrink-0 cursor-col-resize bg-line outline-none ${
              dragging ? "bg-accent" : ""
            }`}
          >
            {/* A wider hit area than the 1px line. */}
            <span className="absolute inset-y-0 -left-1 -right-1 group-hover:bg-accent/40 group-focus-visible:bg-accent/60" />
          </div>
          <aside style={{ width: `${shown}rem` }} className="min-h-0 min-w-0 shrink-0 overflow-y-auto overflow-x-hidden p-4">
            {panel}
          </aside>
        </>
      )}
    </div>
  );
}

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered; the panel still works.
  }
}
