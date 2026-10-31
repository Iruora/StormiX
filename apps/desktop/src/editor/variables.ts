import type { StateEffectType } from "@codemirror/state";
import {
  Decoration,
  hoverTooltip,
  MatchDecorator,
  ViewPlugin,
  type DecorationSet,
  type EditorView,
  type ViewUpdate,
} from "@codemirror/view";

import { describeVariable, type VariableTable } from "../lib/variables";
import { variableClass } from "../views/VarField";

const PATTERN = /\{\{\s*([^{}\s]+)\s*\}\}/g;

/**
 * Colours `{{variables}}` by whether the file, the environment or a step
 * defines them, and shows a variable's value when hovered. `refresh` is the
 * effect the editor sends when the variables change.
 */
export function variableMarks(variables: () => VariableTable, refresh: StateEffectType<null>) {
  const decorator = new MatchDecorator({
    regexp: PATTERN,
    decoration: (m) => Decoration.mark({ class: variableClass(variables(), m[1]) }),
  });

  const marks = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = decorator.createDeco(view);
      }
      update(u: ViewUpdate) {
        const refreshed = u.transactions.some((tr) => tr.effects.some((e) => e.is(refresh)));
        this.decorations = refreshed ? decorator.createDeco(u.view) : decorator.updateDeco(u, this.decorations);
      }
    },
    { decorations: (p) => p.decorations },
  );

  const hover = hoverTooltip((view, pos) => {
    const line = view.state.doc.lineAt(pos);
    for (const m of line.text.matchAll(PATTERN)) {
      const from = line.from + m.index!;
      const to = from + m[0].length;
      if (pos < from || pos > to) continue;
      return {
        pos: from,
        end: to,
        above: true,
        create: () => {
          const dom = document.createElement("div");
          dom.className = "cm-var-tip";
          const info = variables().get(m[1]);
          const [first, ...rest] = describeVariable(m[1], info).split("\n");
          const head = document.createElement("div");
          head.className = info ? "cm-var-tip-value" : "cm-var-tip-missing";
          head.textContent = first;
          dom.append(head);
          if (rest.length) {
            const source = document.createElement("div");
            source.className = "cm-var-tip-source";
            source.textContent = rest.join(" ");
            dom.append(source);
          }
          return { dom };
        },
      };
    }
    return null;
  });

  return [marks, hover];
}
