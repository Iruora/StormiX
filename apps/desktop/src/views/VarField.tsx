import { useLayoutEffect, useRef, type ReactNode } from "react";

import { useVariables, VARIABLE, type VariableTable } from "../lib/variables";
import { useVariableTip } from "./VariableTip";

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
 * the warning colour. Hovering one shows its value and where it comes from,
 * with a button to change it.
 *
 * The field's own text is transparent; a copy behind it, kept scrolled in
 * step, draws the colours. The variable marks sit above the field so they
 * can be hovered; pressing one puts the caret there.
 */
export function VarField({ value, onChange, wrap = "", className = "", multiline, rows, onBlur, ...rest }: Props) {
  const variables = useVariables();
  const field = useRef<HTMLInputElement & HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const tip = useVariableTip(variables);
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
          {paint(value, variables, (name, el) => (el ? tip.show(name, el) : tip.leave()), (index) => {
            const f = field.current;
            if (!f) return;
            f.focus();
            f.setSelectionRange(index, index);
          })}
          {/* Keeps a trailing newline's height, like the textarea does. */}
          {multiline && "​"}
        </div>
      )}
      {tip.element}
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

/**
 * Read-only text with its `{{variables}}` coloured, each one showing its
 * value on hover with a button to change it, as in a field.
 */
export function VarText({ text, className = "" }: { text: string; className?: string }) {
  const variables = useVariables();
  const tip = useVariableTip(variables);
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(VARIABLE)) {
    const start = m.index!;
    if (start > last) out.push(text.slice(last, start));
    const name = m[1];
    out.push(
      <span
        key={start}
        data-variable={name}
        className={`rounded-sm ${variableClass(variables, name)}`}
        onMouseEnter={(e) => tip.show(name, e.currentTarget)}
        onMouseLeave={tip.leave}
      >
        {m[0]}
      </span>,
    );
    last = start + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return (
    <span className={className}>
      {out}
      {tip.element}
    </span>
  );
}
