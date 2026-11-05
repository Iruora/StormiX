# The Flow view

The **Flow** tab draws the test file as a diagram: a rail from **START** to **END** with a card for each step, in the order they run. It is the quickest way to see what a flow does and to change its shape.

<Shot name="flow-after-run" alt="The Flow tab after a run, each step with its result" />

## What a card shows

- The step's number, **method**, name and URL, with its `{{variables}}` coloured. Hover a variable to see or [edit its value](/app/variables-and-scripts).
- **sets**: the variables the step extracts or its scripts set. A long list folds to one line with **+N more**.
- The step's **last result**: its status, timing and whether its checks passed.
- A **warning** when a variable it uses comes from a step that is turned off, set aside, or now runs after it.

Click a card to open the step in the Steps tab.

## Changing the flow

Each card has buttons to move the step up or down, **duplicate** it, **set it aside**, **delete** it (with **Undo**), open its **scripts**, turn its **cache** on or off, and a switch to turn it **on or off**. Between two cards, add a new step.

- **Drag** a card by its grip to reorder it.
- **Turn a step off** to skip it without losing its place. It is not sent by runs or load tests and does not appear in results (`enabled: false` in the file).
- **All steps**, in the header, turns every step on, or, when they are all on, turns them all off. The header also counts how many steps will run.

<Shot name="flow-warning" alt="A warning after moving a step before the one that sets its variable" />

## Set aside

The **Set aside** tray holds steps that are out of the flow but kept in the file: never run, never checked. Drag a card into the tray to set it aside, and back onto the rail to put it back. In YAML they live under `flow.set_aside`. It is handy for an old endpoint you may need again, or a step you are still working on.

<Shot name="flow-set-aside" alt="Dragging a step into the Set aside tray" />

In a narrow window the tray moves under the flow.

## Cached steps

The database icon on a card turns its [response cache](/features/cache) on. A step with a kept response shows **cached**; clear it from the card, or every step's cache with **Clear cache** in the header.

<Shot name="cache-flow" alt="Cached steps in the Flow view" />
