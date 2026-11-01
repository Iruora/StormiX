import { Eye, EyeOff, KeyRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";

/**
 * The secrets an environment names without a value (Bruno's `vars:secret`),
 * to type in before a run: the app's `--var`. Values stay in memory only.
 */
export function EnvSecrets({
  names,
  values,
  onChange,
}: {
  names: string[];
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [reveal, setReveal] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const filled = names.filter((n) => values[n]).length;
  const missing = filled < names.length;

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

  return (
    <div ref={box} className="relative border-r border-line">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        title={missing ? "Some secrets have no value yet" : "Secrets"}
        className={`flex items-center gap-1.5 px-2 py-1 text-xs hover:bg-surface-2 ${missing ? "text-warn" : "text-muted"}`}
      >
        <KeyRound size={13} />
        Secrets {filled}/{names.length}
      </button>
      {open && (
        <div className="absolute right-0 top-full z-40 mt-1 w-72 whitespace-normal rounded-lg border border-line bg-surface p-3 shadow-lg">
          <div className="mb-2 flex items-center">
            <span className="text-xs font-semibold">Secrets</span>
            <button
              type="button"
              onClick={() => setReveal(!reveal)}
              aria-label={reveal ? "Hide the values" : "Show the values"}
              className="ml-auto rounded p-1 text-muted hover:bg-surface-2"
            >
              {reveal ? <EyeOff size={13} /> : <Eye size={13} />}
            </button>
          </div>
          <div className="space-y-2">
            {names.map((name, i) => (
              <label key={name} className="block">
                <span className="mono block text-xs text-muted">{name}</span>
                <input
                  type={reveal ? "text" : "password"}
                  autoFocus={i === 0}
                  autoComplete="off"
                  spellCheck={false}
                  value={values[name] ?? ""}
                  onChange={(e) => onChange(name, e.target.value)}
                  className="mono mt-0.5 w-full rounded-md border border-line bg-surface-2 px-2 py-1 text-xs"
                />
              </label>
            ))}
          </div>
          <p className="mt-2 text-[11px] text-subtle">Kept in memory only, never saved to a file.</p>
        </div>
      )}
    </div>
  );
}
