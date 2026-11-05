# A tour of the app

The Stormix window has three parts: a narrow **sidebar** of icons on the left, the **editor** in the middle, and the **results panel** on the right.

<Shot name="header" alt="The header of the Stormix window" />

## The sidebar

Hover an icon to see its label.

| Icon | Page |
|---|---|
| Document | **Test file**: the editor and results, where you spend most of your time |
| Clock | **Runs**: every run and load test you ran, to reopen, compare and export ([Runs, diffs and reports](/app/runs)) |
| Stack | **Workspaces**: switch between the sets of files you work on ([Workspaces](/app/workspaces)) |
| Gear, at the bottom | **Settings**: the proxy for requests and the keyboard shortcuts |

The app's version is shown under the gear. When a newer version is released, a banner offers to install it and restart; updates are signed and only ones signed with the project's key are installed.

## The header

From left to right:

- **File** menu: new from example, open, save, save as, import a collection (Postman, Bruno, OpenCollection) and **Export as** (Bruno folder, OpenCollection file, Postman collection).
- The **workspace** name and the **file name**, with whether it is saved.
- **Search** (Ctrl K): find any action, environment, proxy mode, step or workspace by typing a few letters.
- The **Environment** menu: the environment in use and how many of its variables you changed. It turns amber while a secret has no value ([Environments](/app/environments)).
- The **proxy** button: System proxy, No proxy or a given proxy ([Proxies](/features/proxies)).
- **Run once** and **Load test**.

<Shot name="file-menu" alt="The File menu" />

## The editor

The tab row switches between three views of the same test file. A change in one shows in the others at once.

- **Steps**: a list of the steps on the left, and a form for the selected one on the right. **Flow settings** at the top of the list holds the flow's name, tracing, load profile and thresholds ([Editing steps and YAML](/app/editing)).
- **Flow**: the steps as a diagram, to reorder, turn off, set aside and cache them ([The Flow view](/app/flow)).
- **YAML**: the whole file in a code editor that checks it as you type ([Editing steps and YAML](/app/editing#the-yaml-editor)).

## The results panel

It has three tabs:

- **Problems**, with a count: errors and warnings in the test file, and anything that did not carry over from an imported collection.
- **Last run**: each step of the last run with its status, timing and checks, and a **Details** button ([Debugging](/app/debugging)). A run opens this tab while it goes and shows which step it is waiting for.
- **Summary**: the steps, the load profile and the thresholds at a glance.

Hide the panel with the toggle at the end of the tab row, and resize it by dragging its edge (arrow keys work too; double-click resets it). While it is hidden, the toggle shows a dot when there are errors or failures. It opens by itself when a run starts.

## Search

Ctrl K (⌘ K on macOS) opens a search box over everything you can do: type `bru` to find the Bruno import, the name of a step to open it, an environment's name to switch to it.

<Shot name="search" alt="Searching actions with Ctrl K" />

## Keyboard shortcuts

| Shortcut | Does |
|---|---|
| Ctrl K | Search actions, steps, environments and workspaces |
| Ctrl O | Open a test file |
| Ctrl S | Save (also inside the YAML and script editors) |
| Ctrl Shift S | Save as |
| F8 | In the YAML editor, jump to the next problem |
| Up, Down | In the Steps list, move between steps |

Use ⌘ instead of Ctrl on macOS. **Settings › Keyboard** lists them too.

## Large text and small windows

The layout follows the system's text size and display scaling. The header and toolbars wrap instead of pushing buttons off screen, and the results panel and editor share a window too small for both.

<Shot name="steps-narrow" alt="The app in a narrow window" narrow />

## While things happen

A strip under the header says what the app is doing, with a progress bar: importing a collection ("37 of 120 files"), opening or saving, running a flow step by step, starting a load test. The button that started it shows a spinner, and buttons that would clash with it wait until it finishes. Large imports run off the window's main thread, so the window never freezes.
