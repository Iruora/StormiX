import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { javascript, javascriptLanguage } from "@codemirror/lang-javascript";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { lintGutter, linter, lintKeymap, type Diagnostic } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  placeholder,
} from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

import type { VariableTable } from "../lib/variables";
import { scriptCompletions, scriptSyntaxErrors, type ScriptPhase } from "./scriptComplete";
import { highlight, theme } from "./YamlEditor";

export type ScriptStatus = { line: number; col: number; errors: number; first: string };

export type ScriptEditorHandle = {
  /** Puts text at the caret, on a line of its own, and focuses the editor. */
  insert: (text: string) => void;
  focus: () => void;
};

type Props = {
  value: string;
  onChange: (text: string) => void;
  phase: ScriptPhase;
  variables: VariableTable;
  onSave?: () => void;
  onStatus?: (status: ScriptStatus) => void;
  label: string;
  hint: string;
};

/**
 * A step script in a JavaScript editor: highlighting, syntax errors as you
 * type, and completion of the sandbox's `bru`, `req`, `res` and `pm`, plus
 * the flow's variable names inside `getVar("…")` and `{{…}}`.
 */
export const ScriptEditor = forwardRef<ScriptEditorHandle, Props>(function ScriptEditor(
  { value, onChange, phase, variables, onSave, onStatus, label, hint },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const latest = useRef({ onChange, variables, onSave, onStatus });
  latest.current = { onChange, variables, onSave, onStatus };
  const errors = useRef<Diagnostic[]>([]);

  useImperativeHandle(ref, () => ({
    insert: (text) => {
      const v = view.current;
      if (!v) return;
      const line = v.state.doc.lineAt(v.state.selection.main.head);
      // On an empty line, in place; otherwise on a new line below.
      const blank = line.text.trim() === "";
      const at = blank ? line.from : line.to;
      const insert = blank ? text : `\n${text}`;
      v.dispatch({
        changes: { from: at, to: blank ? line.to : at, insert },
        selection: { anchor: at + insert.length },
        scrollIntoView: true,
      });
      v.focus();
    },
    focus: () => view.current?.focus(),
  }));

  useEffect(() => {
    const report = (v: EditorView) => {
      const head = v.state.selection.main.head;
      const line = v.state.doc.lineAt(head);
      const first = errors.current[0];
      latest.current.onStatus?.({
        line: line.number,
        col: head - line.from + 1,
        errors: errors.current.length,
        first: first ? `Ln ${v.state.doc.lineAt(first.from).number}: ${first.message}` : "",
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
          highlightActiveLine(),
          highlightSelectionMatches(),
          indentUnit.of("  "),
          EditorState.tabSize.of(2),
          javascript(),
          // Added to the language's own completions: keywords, snippets and local names.
          javascriptLanguage.data.of({ autocomplete: scriptCompletions(phase, () => latest.current.variables) }),
          autocompletion({ icons: false, activateOnTyping: true }),
          syntaxHighlighting(highlight),
          theme,
          placeholder(hint),
          linter(
            (v) => {
              errors.current = scriptSyntaxErrors(v.state);
              report(v);
              return errors.current;
            },
            { delay: 300 },
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
            if (u.docChanged) latest.current.onChange(u.state.doc.toString());
            if (u.selectionSet || u.docChanged) report(u.view);
          }),
          EditorView.contentAttributes.of({ "aria-label": label }),
        ],
      }),
    });
    view.current = created;
    report(created);
    return () => {
      created.destroy();
      view.current = null;
    };
    // Built once per script; the dialog owns the text while it is open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={host} className="h-full min-h-0 overflow-hidden" />;
});
