# Editing steps and YAML

A test file can be edited in three views: **Steps**, **Flow** and **YAML**. They are views of one file, so use whichever suits the change. This page covers Steps and YAML; the [Flow view](/app/flow) has its own page.

## The Steps tab

The Steps tab lists every step with its method on the left and edits the selected one on the right, like Bruno. Up and Down move through the list, and **Add a step** adds one at the end.

<Shot name="steps-list" alt="The Steps tab with a step selected" />

For each step you can set:

- the **method** (each has its own colour) and **URL**;
- **headers**;
- the **body**: none, json, raw, form, multipart or [graphql](/features/graphql). Switching between json and raw keeps the text, and each kind remembers its content;
- the **expected status**;
- whether it **runs** (turned off, it keeps its place but is not sent) and whether its response is **cached** ([Response cache](/features/cache));
- its **scripts**, in a code editor ([Scripts](/app/variables-and-scripts#scripts)).

Extractions and the other assertions are shown on the step and edited in YAML.

### Flow settings

**Flow settings**, at the top of the list, holds what applies to the whole flow:

- the flow's **name**;
- **Trace requests**: off, each request or each flow run ([Tracing](/features/tracing));
- the **load profile**: constant rate, ramping rate with stages, or virtual users;
- the **thresholds**, typed as `p95 < 300ms` and turned into chips.

<Shot name="app-summary" alt="Flow settings with a ramping load profile and thresholds" />

::: tip
Editing in the Steps tab rewrites the YAML, so comments in the YAML are dropped. Keep comments by editing in the YAML tab.
:::

## The YAML editor

The YAML tab shows the whole [test file](/reference/test-file) in a code editor with highlighting, line numbers, folding and search.

- **Errors where they are.** Syntax errors are found on each keystroke. Format errors (an unknown key, a wrong method, a bad value) and warnings are underlined on the key or value they are about.
- **Problems count.** The status bar counts problems; F8 jumps to the next one.
- **Completion** of the format's keys where the cursor is, of methods and thresholds, and of `{{variables}}`: the flow's, extracted ones, the environment's and generated ones like `{{$guid}}`.
- Ctrl S saves.

<Shot name="yaml-errors" alt="A format error underlined in the YAML editor" single />

## Variables in the editors

`{{variables}}` are coloured by where their value comes from: defined in the flow or the environment, set at run time by an extraction or a script, generated, or undefined. Hover one to see its value and its source, including what the last run extracted, and to [edit it](/app/variables-and-scripts).

## Saving

The file name and its saved state are shown in the header. **File › Save** (Ctrl S) writes the test file as `.stormix.yaml`, ready to commit next to your code and run with the [CLI](/reference/cli). An imported collection is not tied to a file until you save it; until then it is kept in your [workspace](/app/workspaces).
