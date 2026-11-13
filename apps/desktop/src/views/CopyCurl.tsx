import { Check, KeyRound, SquareTerminal, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { CurlCommand, Shell, Step } from "../bindings";
import { defaultShell, shellName, useCurlMaker } from "../lib/curl";
import { variablesUsedBy } from "../lib/variables";

type Copied = { shell: Shell; result: CurlCommand; secrets: string[]; at: DOMRect };

/**
 * Copies the step's request as a `curl` command, variables filled in. A
 * note under the button says what was copied: the shell it is quoted for,
 * with the other one a click away, variables that had no value, secrets
 * it holds and anything a run would send differently.
 */
export function CopyCurlButton({ step, variant }: { step: Step; variant: "icon" | "chip" }) {
  const maker = useCurlMaker();
  const [copied, setCopied] = useState<Copied | null>(null);
  const [error, setError] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const noteRef = useRef<HTMLDivElement>(null);
  const hovered = useRef(false);

  const copy = async (shell: Shell) => {
    if (!maker) return;
    setError(null);
    try {
      const result = await maker.make(step, shell);
      await navigator.clipboard.writeText(result.command);
      const used = new Set(variablesUsedBy(step));
      setCopied({
        shell,
        result,
        secrets: maker.secrets.filter((s) => used.has(s) && !result.missing.includes(s)),
        at: buttonRef.current!.getBoundingClientRect(),
      });
    } catch (e) {
      setCopied(null);
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  // The note closes on a click elsewhere, Escape, scrolling, or after a while
  // unless the pointer is on it.
  const open = copied !== null || error !== null;
  useEffect(() => {
    if (!open) return;
    const close = () => {
      setCopied(null);
      setError(null);
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!noteRef.current?.contains(t) && !buttonRef.current?.contains(t)) close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    const timer = window.setInterval(() => !hovered.current && close(), 8000);
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", close, true);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", close, true);
    };
  }, [open, copied]);

  if (!maker) return null;
  const label = "Copy as cURL, with variables filled in";
  const done = copied !== null;
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        aria-label="Copy as cURL"
        title={label}
        onClick={() => copy(defaultShell())}
        className={
          variant === "icon"
            ? `shrink-0 rounded p-1 ${done ? "text-ok" : "text-subtle hover:bg-surface-2 hover:text-ink"}`
            : `flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6875rem] ${
                done ? "text-ok" : "text-subtle hover:bg-surface-2 hover:text-ink"
              }`
        }
      >
        {done ? <Check size={variant === "icon" ? "0.8125rem" : "0.75rem"} /> : <SquareTerminal size={variant === "icon" ? "0.8125rem" : "0.75rem"} />}
        {variant === "chip" && (done ? "copied" : "curl")}
      </button>
      {open && (
        <div
          ref={noteRef}
          role="status"
          onPointerEnter={() => (hovered.current = true)}
          onPointerLeave={() => (hovered.current = false)}
          style={placeBelow(copied?.at ?? buttonRef.current?.getBoundingClientRect())}
          className="fixed z-[60] w-80 max-w-[calc(100vw-1rem)] space-y-1.5 overflow-y-auto rounded-md border border-line bg-surface px-2.5 py-2 text-xs shadow-lg"
        >
          {error ? (
            <div className="flex items-start gap-1.5 text-bad">
              <TriangleAlert size="0.8125rem" className="mt-px shrink-0" /> Could not build the command: {error}
            </div>
          ) : (
            copied && <CopiedNote copied={copied} onOther={(shell) => copy(shell)} />
          )}
        </div>
      )}
    </>
  );
}

function CopiedNote({ copied, onOther }: { copied: Copied; onOther: (shell: Shell) => void }) {
  const { result, shell, secrets } = copied;
  const other: Shell = shell === "cmd" ? "posix" : "cmd";
  return (
    <>
      <div className="flex items-center gap-1.5">
        <Check size="0.8125rem" className="shrink-0 text-ok" />
        <span className="flex-1 text-ink">Copied as cURL for {shellName(shell)}</span>
        <button
          type="button"
          onClick={() => onOther(other)}
          className="shrink-0 rounded px-1 font-medium text-accent hover:bg-accent-soft"
        >
          for {shellName(other)}
        </button>
      </div>
      {result.missing.length > 0 && (
        <div className="flex items-start gap-1.5 text-warn">
          <TriangleAlert size="0.8125rem" className="mt-px shrink-0" />
          <span>
            {result.missing.map((m, i) => (
              <span key={m}>
                {i > 0 && ", "}
                <span className="mono">{`{{${m}}}`}</span>
              </span>
            ))}{" "}
            {result.missing.length === 1 ? "has" : "have"} no value yet, so {result.missing.length === 1 ? "it stays" : "they stay"}{" "}
            as written. Run the flow once to fill in what earlier steps extract.
          </span>
        </div>
      )}
      {secrets.length > 0 && (
        <div className="flex items-start gap-1.5 text-muted">
          <KeyRound size="0.8125rem" className="mt-px shrink-0" />
          <span>
            Holds the value of the secret{secrets.length > 1 ? "s" : ""}{" "}
            <span className="mono">{secrets.join(", ")}</span>. Keep it out of tickets and chats.
          </span>
        </div>
      )}
      {result.notes.map((n) => (
        <div key={n} className="flex items-start gap-1.5 text-muted">
          <TriangleAlert size="0.8125rem" className="mt-px shrink-0" /> {n}
        </div>
      ))}
    </>
  );
}

/**
 * Under the button, or above it when there is more room there, kept inside
 * the window; a long note scrolls.
 */
function placeBelow(at: DOMRect | undefined): React.CSSProperties {
  if (!at) return { display: "none" };
  const width = Math.min(320, window.innerWidth - 16);
  const left = Math.max(8, Math.min(at.right - width, window.innerWidth - width - 8));
  const below = window.innerHeight - at.bottom - 14;
  const above = at.top - 14;
  return below >= 160 || below >= above
    ? { top: at.bottom + 6, left, maxHeight: below }
    : { bottom: window.innerHeight - at.top + 6, left, maxHeight: above };
}
