import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

import { describeVariable, useVariables, VARIABLE, type VariableTable } from "../lib/variables";

type Props = {
  value: string;
  /** Size and placement of the field, like `flex-1` or `w-2/5`. */
  wrap?: string;
  onChange: (value: string) => void;
  className?: string;
  placeholder?: string;
  "aria-label"?: string;
  multiline?: boolean;
  rows?: number;
  onBlur?: () => void;
};

/**
 * A text input or textarea that colours its `{{variables}}`: known ones in
 * the accent colour, ones only a run can fill in another, unknown ones in
 * the warning colour. Hovering one shows its value and where it comes from.
 *
 * The field's own text is transparent; a copy behind it, kept scrolled in
 * step, draws the colours. The variable marks sit above the field so they
 * can be hovered; pressing one puts the caret there.
 */
export function VarField({ value, onChange, wrap = "", className = "", multiline, rows, onBlur, ...rest }: Props) {
  const variables = useVariables();
  const field = useRef<HTMLInputElement & HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ name: string; x: number; y: number; above: boolean } | null>(null);
  const hasVariables = value.includes("{{");

  const sync = () => {
    if (field.current && mirror.current) {
      mirror.current.scrollLeft = field.current.scrollLeft;
      mirror.current.scrollTop = field.current.scrollTop;
    }
  };
  useLayoutEffect(sync);

  const common = {
    ref: field,
    value,
    spellCheck: false,
    placeholder: rest.placeholder,
    "aria-label": rest["aria-label"],
    onChange: (e: { target: { value: string } }) => onChange(e.target.value),
    onScroll: sync,
    onKeyUp: sync,
    onSelect: sync,
    onBlur,
    className: `${className} w-full ${hasVariables ? "text-transparent caret-ink selection:bg-accent/25" : ""}`,
  };

  return (
    <div className={`relative min-w-0 ${multiline ? "block" : "flex"} ${wrap}`}>
      {multiline ? <textarea {...common} rows={rows} /> : <input {...common} />}
      {hasVariables && (
        <div
          ref={mirror}
          aria-hidden
          className={`${className} pointer-events-none absolute inset-0 overflow-hidden ${
            multiline ? "whitespace-pre-wrap break-words" : "flex items-center whitespace-pre"
          }`}
          style={{ background: "transparent", borderColor: "transparent", resize: "none" }}
        >
          {paint(value, variables, (name, el) => {
            if (!el) return setHover(null);
            const r = el.getBoundingClientRect();
            // Near the bottom of the window, the tip opens upwards.
            const above = r.bottom + 72 > window.innerHeight;
            setHover({ name, x: r.left, y: above ? r.top - 4 : r.bottom + 4, above });
          }, (index) => {
            const f = field.current;
            if (!f) return;
            f.focus();
            f.setSelectionRange(index, index);
          })}
          {/* Keeps a trailing newline's height, like the textarea does. */}
          {multiline && "​"}
        </div>
      )}
      {hover &&
        createPortal(
          <VariableTip name={hover.name} variables={variables} x={hover.x} y={hover.y} above={hover.above} />,
          document.body,
        )}
    </div>
  );
}

function paint(
  text: string,
  variables: VariableTable,
  onHover: (name: string, el: HTMLElement | null) => void,
  onPress: (index: number) => void,
): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(VARIABLE)) {
    const start = m.index!;
    if (start > last) out.push(<span key={`t${last}`} className="text-ink">{text.slice(last, start)}</span>);
    const name = m[1];
    out.push(
      <span
        key={`v${start}`}
        data-variable={name}
        className={`pointer-events-auto cursor-text rounded-sm ${variableClass(variables, name)}`}
        onMouseEnter={(e) => onHover(name, e.currentTarget)}
        onMouseLeave={() => onHover(name, null)}
        onMouseDown={(e) => {
          e.preventDefault();
          onPress(start + m[0].length);
        }}
      >
        {m[0]}
      </span>,
    );
    last = start + m[0].length;
  }
  if (last < text.length) out.push(<span key={`t${last}`} className="text-ink">{text.slice(last)}</span>);
  return out;
}

/** The same classes the YAML editor uses, so a variable looks alike everywhere. */
export function variableClass(variables: VariableTable, name: string): string {
  const info = variables.get(name);
  if (!info) return "var-missing";
  return info.kind === "generated" ? "var-generated" : info.kind === "runtime" ? "var-runtime" : "var-defined";
}

function VariableTip({
  name,
  variables,
  x,
  y,
  above,
}: {
  name: string;
  variables: VariableTable;
  x: number;
  y: number;
  above: boolean;
}) {
  const info = variables.get(name);
  const [first, ...more] = describeVariable(name, info).split("\n");
  return (
    <div
      role="tooltip"
      className="pointer-events-none fixed z-[60] max-w-sm rounded-md border border-line bg-surface px-2.5 py-1.5 text-xs shadow-lg"
      style={{ left: Math.min(x, window.innerWidth - 340), top: y, transform: above ? "translateY(-100%)" : undefined }}
    >
      {info?.value != null ? (
        <>
          <div className="mono break-all">
            <span className={variableClass(variables, name)}>{`{{${name}}}`}</span>
            <span className="text-subtle"> = </span>
            <span className="text-ink">{info.value}</span>
          </div>
          <div className="mt-0.5 text-subtle">{more.join(" ")}</div>
        </>
      ) : (
        <div className={info ? "text-muted" : "text-warn"}>{first}</div>
      )}
    </div>
  );
}
