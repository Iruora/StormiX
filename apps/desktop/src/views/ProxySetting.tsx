import { Globe } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { ProxyChoice } from "../bindings";
import { describeProxy, useProxy } from "../lib/proxy";

/** Like Bruno's proxy preference: the system's, none, or one address. Kept across launches. */
export function ProxySetting() {
  const { current, change } = useProxy();
  const [url, setUrl] = useState("");
  useEffect(() => {
    if (current && typeof current === "object") setUrl(current.url);
  }, [current]);
  const mode =
    current === null ? null : typeof current === "object" ? "url" : current;
  const pick = (choice: ProxyChoice) => change.mutate(choice);
  const options: { id: "system" | "none" | "url"; label: string }[] = [
    { id: "system", label: "System proxy" },
    { id: "none", label: "No proxy" },
    { id: "url", label: "Proxy" },
  ];
  return (
    <div className="text-[0.8125rem]">
      <div className="flex flex-wrap items-center gap-2">
        <div
          role="radiogroup"
          aria-label="Proxy for requests"
          className="flex rounded-md border border-line p-0.5"
        >
          {options.map((o) => (
            <button
              key={o.id}
              type="button"
              role="radio"
              aria-checked={mode === o.id}
              onClick={() =>
                o.id === "url"
                  ? url.includes("://") && pick({ url })
                  : pick(o.id)
              }
              className={`rounded px-2 py-0.5 text-xs font-medium ${
                mode === o.id
                  ? "bg-accent-soft text-accent"
                  : "text-muted hover:text-ink"
              }`}
            >
              {o.label}
            </button>
          ))}
        </div>
        <form
          className="flex min-w-0 flex-1 basis-56 items-center gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            if (url.includes("://")) pick({ url: url.trim() });
          }}
        >
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="http://proxy.example:8080"
            aria-label="Proxy address"
            className="mono min-w-0 flex-1 rounded-md border border-line bg-surface px-2 py-0.5 text-xs"
          />
          <button
            type="submit"
            disabled={!url.includes("://")}
            className="rounded-md border border-line px-2 py-0.5 text-xs text-muted hover:text-ink disabled:opacity-50"
          >
            Use
          </button>
        </form>
      </div>
      <p className="mt-1.5 text-xs text-muted">
        {mode === "none"
          ? "Requests go straight to every server, like Bruno with its proxy off."
          : mode === "url"
            ? `Every request goes through ${current && describeProxy(current)}.`
            : "Requests follow HTTPS_PROXY and NO_PROXY, then the Windows or macOS proxy settings."}{" "}
        Applies to every run from now on, and is kept when Stormix restarts.
      </p>
      {change.error && (
        <p className="mt-1 text-xs text-bad">{change.error.message}</p>
      )}
    </div>
  );
}

/**
 * The proxy setting from the header, so it can be changed before any run:
 * shows the mode in use and opens the choice in a popover.
 */
export function ProxyButton() {
  const { current } = useProxy();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const escape = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const label =
    current === null
      ? "Proxy"
      : current === "system"
        ? "System proxy"
        : current === "none"
          ? "No proxy"
          : "Proxy";
  return (
    <div ref={box} className="relative">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        title={
          current && typeof current === "object"
            ? `Requests go through ${describeProxy(current)}`
            : "Choose the proxy requests go through"
        }
        className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2 hover:text-ink"
      >
        <Globe size="0.8125rem" />
        {label}
      </button>
      {open && (
        <div className="absolute right-0 top-full z-40 mt-1 w-[26rem] max-w-[calc(100vw-2rem)] whitespace-normal rounded-lg border border-line bg-surface p-3 shadow-lg">
          <p className="mb-2 text-xs font-semibold">Proxy for requests</p>
          <ProxySetting />
        </div>
      )}
    </div>
  );
}
