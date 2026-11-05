# Workspaces

A workspace is what you have open, saved as you work. Close the app and it reopens where you left off, unsaved changes included.

## What a workspace keeps

- the test file, or the editor text when it is not saved to a file yet (an imported collection, say), with changes not saved to the file;
- every environment in the list and the one chosen;
- the editor tab and page you were on;
- the steps you left out of the last load test.

**Not kept:** secret values, and variable changes made for this session only. Type secrets in again after a restart.

When a workspace reopens, its file is read again from disk, with the environments beside it, so changes made outside the app show up. If the file is gone, the copy kept with the workspace opens instead.

## The Workspaces page

The stack icon in the sidebar, or the workspace name in the header, opens the list of workspaces with their file, environment and when they were last saved.

<Shot name="workspaces" alt="The Workspaces page" />

- **Open** one to switch to it.
- **Rename** one. Until you do, it is named after its file or flow.
- **Delete** one (the test file stays on disk).
- **New workspace** starts another from the example.

Use one workspace per API or per project, and switch between them without losing your place.

## Where they are stored

Workspaces are kept in `workspaces.json` in the app's data folder. Run history is next to it in `runs.db`.
