import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

const MIN = 280;
const DEFAULT = 440;
const STEP = 24;

/** Whether the right panel is open and how wide it is, remembered between launches. */
export function usePanel() {
  const [open, setOpenState] = useState(() => stored("stormix.panel.open") !== "false");
  const [width, setWidthState] = useState(() => {
    const n = Number(stored("stormix.panel.width"));
    return Number.isFinite(n) && n >= MIN ? n : DEFAULT;
  });
  const setOpen = useCallback((next: boolean) => {
    setOpenState(next);
    store("stormix.panel.open", String(next));
  }, []);
  const setWidth = useCallback((next: number) => {
    setWidthState(next);
    store("stormix.panel.width", String(Math.round(next)));
  }, []);
  return { open, setOpen, width, setWidth };
}

/**
 * The main content with a panel on its right that can be closed and
 * resized by dragging its left edge. The panel never takes more than
 * 70% of the space, nor less than 280px.
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
  const [max, setMax] = useState(Infinity);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setMax(Math.max(MIN, el.clientWidth * 0.7)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const clamp = (n: number) => Math.min(max, Math.max(MIN, n));
  const shown = clamp(width);

  const startDrag = (e: React.PointerEvent) => {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    setDragging(true);
    const move = (ev: PointerEvent) => {
      const right = box.current?.getBoundingClientRect().right ?? 0;
      onWidth(clamp(right - ev.clientX));
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
    if (e.key === "ArrowLeft") onWidth(clamp(shown + STEP));
    else if (e.key === "ArrowRight") onWidth(clamp(shown - STEP));
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
            aria-valuenow={Math.round(shown)}
            tabIndex={0}
            onPointerDown={startDrag}
            onDoubleClick={() => onWidth(clamp(DEFAULT))}
            onKeyDown={onKey}
            title="Drag to resize, double-click to reset"
            className={`group relative w-px shrink-0 cursor-col-resize bg-line outline-none ${
              dragging ? "bg-accent" : ""
            }`}
          >
            {/* A wider hit area than the 1px line. */}
            <span className="absolute inset-y-0 -left-1 -right-1 group-hover:bg-accent/40 group-focus-visible:bg-accent/60" />
          </div>
          <aside style={{ width: shown }} className="min-h-0 shrink-0 overflow-y-auto p-4">
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
