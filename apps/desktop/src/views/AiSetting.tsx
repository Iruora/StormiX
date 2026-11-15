import { CircleCheck, CircleMinus, RefreshCw } from "lucide-react";

import type { Provider } from "../bindings";
import { INSTALL_URL, PROVIDER_LABEL, useAiProviders, useAiSettings } from "../lib/ai";
import { Spinner } from "./Busy";

const PROVIDERS: Provider[] = ["claude", "copilot"];

/**
 * Which AI command line explains results: found on this computer, with a
 * path to set by hand and a model to pick.
 */
export function AiSetting() {
  const [settings, change] = useAiSettings();
  const { found, active, loading, refresh } = useAiProviders();
  return (
    <div className="space-y-3 text-[0.8125rem]">
      <p className="text-xs text-muted">
        Explain uses the AI command line you already have, signed in with your own account, so Stormix needs no API
        key and opens no connection of its own. Secrets, auth headers and tokens are hidden before anything is sent,
        and the AI gets no tools.
      </p>
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={settings.enabled}
          onChange={(e) => change({ enabled: e.target.checked })}
          className="accent-[var(--accent)]"
        />
        Show Explain buttons on results
      </label>

      <div className={settings.enabled ? "space-y-3" : "pointer-events-none space-y-3 opacity-50"}>
        <div className="overflow-hidden rounded-md border border-line">
          {PROVIDERS.map((p) => {
            const f = found.find((x) => x.provider === p);
            const chosen = settings.provider === p || (settings.provider == null && active?.provider === p);
            return (
              <div key={p} className="flex flex-wrap items-center gap-x-3 gap-y-1.5 border-t border-line px-3 py-2 first:border-t-0">
                <label className="flex min-w-56 flex-1 items-center gap-2.5">
                  <input
                    type="radio"
                    name="ai-provider"
                    disabled={!f}
                    checked={!!f && chosen}
                    onChange={() => change({ provider: p })}
                    className="accent-[var(--accent)]"
                  />
                  <span className="flex min-w-0 flex-col">
                  <span className="font-medium whitespace-nowrap">{PROVIDER_LABEL[p]}</span>
                  {loading ? (
                    <Spinner className="text-muted" />
                  ) : f ? (
                    <span className="flex min-w-0 items-center gap-1 text-xs text-ok" title={f.path}>
                      <CircleCheck size="0.75rem" /> <span className="truncate">{f.version ?? "found"}</span>
                    </span>
                  ) : (
                    <span className="flex items-center gap-1 text-xs text-subtle">
                      <CircleMinus size="0.75rem" /> not found ·{" "}
                      <a className="text-accent underline" href={INSTALL_URL[p]} target="_blank" rel="noreferrer">
                        install
                      </a>
                    </span>
                  )}
                  </span>
                </label>
                <input
                  type="text"
                  spellCheck={false}
                  placeholder={f ? f.path : `Path to ${p === "claude" ? "claude" : "copilot"}, if not on PATH`}
                  defaultValue={settings.paths[p] ?? ""}
                  onBlur={(e) => {
                    if ((settings.paths[p] ?? "") !== e.target.value.trim())
                      change({ paths: { ...settings.paths, [p]: e.target.value.trim() } });
                  }}
                  className="mono min-w-0 flex-[1.4] basis-56 rounded border border-line bg-surface-2 px-2 py-1 text-xs"
                />
              </div>
            );
          })}
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="radio"
              name="ai-provider-auto"
              checked={settings.provider == null}
              onChange={() => change({ provider: null })}
              className="accent-[var(--accent)]"
            />
            Use the first one found
          </label>
          <label className="flex items-center gap-2 text-xs">
            Model
            <input
              type="text"
              spellCheck={false}
              placeholder="the provider's default"
              defaultValue={settings.model}
              onBlur={(e) => e.target.value.trim() !== settings.model && change({ model: e.target.value.trim() })}
              className="mono w-44 rounded border border-line bg-surface-2 px-2 py-1 text-xs"
            />
          </label>
          <button
            type="button"
            onClick={refresh}
            className="ml-auto flex items-center gap-1 rounded border border-line bg-surface px-2 py-0.5 text-xs text-muted hover:text-ink"
          >
            <RefreshCw size="0.75rem" /> Look again
          </button>
        </div>
        <p className="text-xs text-subtle">
          Not signed in yet? Run <span className="mono">claude</span> or <span className="mono">copilot</span> once in a
          terminal and sign in. On a company network the AI command line uses its own proxy settings, not the proxy
          chosen above for requests.
        </p>
      </div>
    </div>
  );
}
