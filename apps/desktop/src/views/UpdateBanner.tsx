import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Download, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";

/**
 * Looks for a newer release once at launch. When there is one, offers to
 * download it, install it and restart. A failed check (offline, rate
 * limited) stays silent: the app works the same without it.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [state, setState] = useState<"idle" | "downloading" | "error">("idle");
  const [progress, setProgress] = useState<{ done: number; total: number | null }>({ done: 0, total: null });
  const [error, setError] = useState("");
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    // Development builds have no updater plugin behind them.
    if (import.meta.env.DEV) return;
    check()
      .then((found) => found && setUpdate(found))
      .catch(() => {});
  }, []);

  if (!update || hidden) return null;

  const install = async () => {
    setState("downloading");
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") setProgress({ done: 0, total: event.data.contentLength ?? null });
        if (event.event === "Progress") setProgress((p) => ({ ...p, done: p.done + event.data.chunkLength }));
      });
      await relaunch();
    } catch (e) {
      setError(String(e));
      setState("error");
    }
  };

  const percent = progress.total ? Math.round((progress.done / progress.total) * 100) : null;
  return (
    <div className="flex items-center gap-3 border-b border-accent/30 bg-accent-soft px-4 py-2 text-[0.8125rem]">
      <Download size="0.9375rem" className="shrink-0 text-accent" />
      <span className="min-w-0 flex-1 truncate">
        {state === "error" ? (
          <span className="text-bad">The update could not be installed: {error}</span>
        ) : state === "downloading" ? (
          <>Downloading Stormix {update.version}{percent != null && ` · ${percent}%`}. The app restarts when it is done.</>
        ) : (
          <>
            <b>Stormix {update.version}</b> is available. You have {update.currentVersion}.
          </>
        )}
      </span>
      {state !== "downloading" && (
        <a
          href={`https://github.com/Iruora/StormiX/releases/tag/v${update.version}`}
          target="_blank"
          rel="noreferrer"
          className="shrink-0 text-xs text-accent hover:underline"
        >
          What's new
        </a>
      )}
      {state === "idle" && (
        <button
          type="button"
          onClick={install}
          className="shrink-0 rounded-md bg-accent px-2.5 py-1 text-xs font-semibold text-white hover:opacity-90"
        >
          Install and restart
        </button>
      )}
      {state === "downloading" && <LoaderCircle size="0.9375rem" className="shrink-0 animate-spin text-accent" />}
      {state !== "downloading" && (
        <button type="button" aria-label="Later" title="Later" onClick={() => setHidden(true)} className="rounded p-1 text-muted hover:bg-surface">
          <X size="0.875rem" />
        </button>
      )}
    </div>
  );
}
