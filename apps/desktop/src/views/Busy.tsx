import { LoaderCircle } from "lucide-react";
import type { ReactNode } from "react";

/** The spinning circle shown while something is under way. */
export function Spinner({ size = "0.8125rem", className = "" }: { size?: string; className?: string }) {
  return <LoaderCircle size={size} aria-hidden className={`shrink-0 animate-spin ${className}`} />;
}

/**
 * A thin bar: filled to `value` (0 to 1) when how far along is known,
 * otherwise a segment sweeping across.
 */
export function ProgressBar({ value, className = "" }: { value: number | null; className?: string }) {
  const known = value != null && Number.isFinite(value);
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={known ? Math.round(value * 100) : undefined}
      className={`relative h-1 overflow-hidden rounded-full bg-surface-2 ${className}`}
    >
      {known ? (
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-200"
          style={{ width: `${Math.min(1, Math.max(0, value)) * 100}%` }}
        />
      ) : (
        <div className="progress-sweep" />
      )}
    </div>
  );
}

/** What the app is doing now, as the activity strip shows it. */
export type Activity = {
  /** Like "Importing the Bruno collection payments". */
  label: string;
  /** Like the file being read. */
  detail?: string;
  /** When known, how far along: like 12 of 80 files. */
  done?: number;
  total?: number;
  unit?: string;
};

/**
 * A strip under the toolbar saying what is under way, with a progress bar.
 * Takes no room when nothing is.
 */
export function ActivityStrip({ activity }: { activity: Activity | null }) {
  // Kept in the page while idle so screen readers hear each change.
  return (
    <div role="status" aria-live="polite" className={activity ? "border-b border-line bg-surface" : "sr-only"}>
      {activity && (
        <>
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 px-5 py-1.5 text-xs">
            <Spinner className="text-accent" />
            <span className="font-medium text-ink">{activity.label}…</span>
            {activity.detail && (
              <span className="mono min-w-0 flex-1 basis-32 truncate text-muted" title={activity.detail}>
                {activity.detail}
              </span>
            )}
            {activity.total != null && activity.total > 0 && (
              <span className="ml-auto shrink-0 tabular-nums text-muted">
                {activity.done ?? 0} of {activity.total} {activity.unit ?? ""}
              </span>
            )}
          </div>
          <ProgressBar
            className="rounded-none"
            value={activity.total ? (activity.done ?? 0) / activity.total : null}
          />
        </>
      )}
    </div>
  );
}

/** A small button that shows a spinner in place of its icon and is disabled while `busy`. */
export function BusyIcon({ busy, icon }: { busy?: boolean; icon?: ReactNode }) {
  return busy ? <Spinner /> : <>{icon}</>;
}

/** Centered spinner and text, for a page or pane that is loading. */
export function Loading({ children }: { children: ReactNode }) {
  return (
    <div role="status" className="flex items-center gap-2 text-muted">
      <Spinner size="0.875rem" className="text-accent" />
      {children}
    </div>
  );
}
