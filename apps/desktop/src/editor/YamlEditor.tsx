import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { yaml } from "@codemirror/lang-yaml";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import { lintGutter, linter, lintKeymap, nextDiagnostic, type Diagnostic } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { EditorState, StateEffect } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { Validation } from "../bindings";
import type { VariableTable } from "../lib/variables";
import { testFileCompletions } from "./complete";
import { variableMarks } from "./variables";
import { diagnose } from "./diagnostics";

type Props = {
  value: string;
  onChange: (text: string) => void;
  /** The validator's result for (about) this text; its issues are shown inline. */
  validation?: Validation;
  /** What each `{{variable}}` stands for: coloured in the text, shown on hover. */
  variables?: VariableTable;
  onSave?: () => void;
};

/**
 * The test file as YAML, with highlighting, folding, completion of the
 * format's keys, and problems underlined where they are as you type.
 */
export function YamlEditor({ value, onChange, validation, variables = EMPTY, onSave }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  // The editor is built once; these let its callbacks see the latest props.
  const latest = useRef({ onChange, validation, variables, onSave });
  latest.current = { onChange, validation, variables, onSave };
  // Set while text from outside is put in, so it is not echoed back as an edit.
  const external = useRef(false);
  const [status, setStatus] = useState({ line: 1, col: 1, errors: 0, warnings: 0, first: "" });

  useEffect(() => {
    const report = (v: EditorView, diagnostics?: Diagnostic[]) => {
      const head = v.state.selection.main.head;
      const line = v.state.doc.lineAt(head);
      setStatus((s) => {
        const next = { ...s, line: line.number, col: head - line.from + 1 };
        if (diagnostics) {
          const errors = diagnostics.filter((d) => d.severity === "error");
          next.errors = errors.length;
          next.warnings = diagnostics.length - errors.length;
          const first = errors[0] ?? diagnostics[0];
          next.first = first ? `Ln ${v.state.doc.lineAt(first.from).number}: ${first.message}` : "";
        }
        return next;
      });
    };
    const created = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          foldGutter(),
          lintGutter(),
          highlightSpecialChars(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          rectangularSelection(),
          highlightActiveLine(),
          highlightSelectionMatches(),
          indentUnit.of("  "),
          EditorState.tabSize.of(2),
          yaml(),
          syntaxHighlighting(highlight),
          variableMarks(() => latest.current.variables, revalidated),
          theme,
          autocompletion({
            override: [testFileCompletions(() => [...latest.current.variables.keys()])],
            icons: false,
          }),
          linter(
            (v) => {
              const diagnostics = diagnose(v.state.doc.toString(), latest.current.validation);
              report(v, diagnostics);
              return diagnostics;
            },
            {
              delay: 150,
              needsRefresh: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(revalidated))),
            },
          ),
          keymap.of([
            {
              key: "Mod-s",
              preventDefault: true,
              run: () => {
                latest.current.onSave?.();
                return true;
              },
            },
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...searchKeymap,
            ...historyKeymap,
            ...foldKeymap,
            ...completionKeymap,
            ...lintKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged && !external.current) latest.current.onChange(u.state.doc.toString());
            if (u.selectionSet || u.docChanged) report(u.view);
          }),
          EditorView.contentAttributes.of({ "aria-label": "Test file YAML" }),
        ],
      }),
    });
    view.current = created;
    return () => {
      created.destroy();
      view.current = null;
    };
    // Built once; `value` changes from outside are applied below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Text replaced from outside: opened, imported or rewritten by the steps editor.
  useEffect(() => {
    const v = view.current;
    if (!v || v.state.doc.toString() === value) return;
    external.current = true;
    try {
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
    } finally {
      external.current = false;
    }
  }, [value]);

  // A new validator result: place its issues without waiting for a keystroke.
  useEffect(() => {
    view.current?.dispatch({ effects: revalidated.of(null) });
  }, [validation, variables]);

  const tone = status.errors > 0 ? "text-bad" : status.warnings > 0 ? "text-warn" : "text-ok";
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div ref={host} className="min-h-0 flex-1 overflow-hidden" />
      <div className="flex items-center gap-3 border-t border-line bg-surface px-3 py-1 text-xs text-muted">
        <button
          type="button"
          title={status.errors + status.warnings > 0 ? "Go to the next problem (F8)" : undefined}
          onClick={() => {
            if (view.current) {
              nextDiagnostic(view.current);
              view.current.focus();
            }
          }}
          className={`flex min-w-0 items-center gap-1.5 ${tone}`}
        >
          {status.errors > 0 ? (
            <CircleAlert size={13} />
          ) : status.warnings > 0 ? (
            <TriangleAlert size={13} />
          ) : (
            <CircleCheck size={13} />
          )}
          <span className="shrink-0">
            {status.errors + status.warnings === 0
              ? "No problems"
              : [count(status.errors, "error"), count(status.warnings, "warning")].filter(Boolean).join(", ")}
          </span>
          {status.first && <span className="min-w-0 truncate text-muted">· {status.first}</span>}
        </button>
        <span className="mono ml-auto shrink-0">
          Ln {status.line}, Col {status.col}
        </span>
        <span className="shrink-0">YAML</span>
      </div>
    </div>
  );
}

const EMPTY: VariableTable = new Map();

/** The validator answered or the variables changed; refresh without waiting for a keystroke. */
const revalidated = StateEffect.define<null>();

function count(n: number, noun: string): string {
  return n === 0 ? "" : `${n} ${noun}${n === 1 ? "" : "s"}`;
}

const highlight = HighlightStyle.define([
  { tag: [t.propertyName, t.definition(t.propertyName)], color: "var(--syn-key)" },
  { tag: [t.string, t.special(t.string), t.content], color: "var(--syn-string)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.keyword, t.typeName, t.labelName, t.meta], color: "var(--syn-meta)" },
  { tag: [t.punctuation, t.separator, t.squareBracket, t.brace], color: "var(--muted)" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "var(--surface)", color: "var(--ink)", fontSize: "13px" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "20px" },
  ".cm-content": { padding: "12px 0", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0 12px 0 6px" },
  ".cm-gutters": { backgroundColor: "var(--surface-2)", color: "var(--subtle)", border: "none", borderRight: "1px solid var(--line)" },
  ".cm-activeLineGutter": { backgroundColor: "var(--accent-soft)", color: "var(--accent)" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--accent-soft) 45%, transparent)" },
  ".cm-cursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent) !important",
  },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in srgb, var(--bolt) 22%, transparent)" },
  ".cm-matchingBracket": { backgroundColor: "var(--accent-soft)", outline: "1px solid var(--accent)" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px", cursor: "pointer" },
  ".cm-lintRange-error": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--bad)",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-lintRange-warning": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--warn)",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-lint-marker": { width: "0.9em", height: "0.9em" },
  ".cm-tooltip": {
    backgroundColor: "var(--surface)",
    color: "var(--ink)",
    border: "1px solid var(--line)",
    borderRadius: "8px",
    boxShadow: "0 8px 24px rgb(0 0 0 / 0.18)",
    overflow: "hidden",
  },
  ".cm-tooltip-lint": { padding: "0" },
  ".cm-diagnostic": { padding: "6px 10px", borderLeftWidth: "3px", fontSize: "12.5px", maxWidth: "460px" },
  ".cm-diagnostic-error": { borderLeftColor: "var(--bad)" },
  ".cm-diagnostic-warning": { borderLeftColor: "var(--warn)" },
  ".cm-diagnosticSource": { fontFamily: "var(--font-mono)", color: "var(--subtle)", fontSize: "11px", marginTop: "2px" },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--font-mono)", fontSize: "12.5px", maxHeight: "16em" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 10px !important" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--accent-soft)", color: "var(--accent)" },
  ".cm-completionInfo": { padding: "6px 10px", fontSize: "12px", maxWidth: "320px" },
  ".cm-panels": { backgroundColor: "var(--surface-2)", color: "var(--ink)", borderColor: "var(--line)" },
  ".cm-panel input, .cm-panel button": { fontSize: "12px" },
  ".cm-var-tip": { padding: "6px 10px", fontSize: "12px", maxWidth: "380px" },
  ".cm-var-tip-value": { fontFamily: "var(--font-mono)", wordBreak: "break-all" },
  ".cm-var-tip-missing": { color: "var(--warn)" },
  ".cm-var-tip-source": { color: "var(--subtle)", marginTop: "2px" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in srgb, var(--bolt) 30%, transparent)" },
});
