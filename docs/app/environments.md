# Environments and secrets

An environment is a set of variables for one place your API runs: `local`, `staging`, `production`. Switching environment changes every `{{variable}}` it defines, without touching the test file.

## The Environment menu

The **Environment** button in the header shows the environment in use, and how many of its variables you changed for this session. It turns amber while a secret has no value. It opens:

- the list of loaded **environments**, to switch between them;
- **Variables**: see and change the chosen environment's values;
- **Secrets**: type in the values of its secrets;
- **Import environment**: add one from a file;
- **Remove**: take the chosen one out of the list.

<Shot name="env-menu" alt="The Environment menu" />

## Where environments come from

- **With a collection.** Importing a Bruno collection or OpenCollection loads its environments. For a Postman collection, add its environment exports with **Import environment**.
- **From a file.** **Import environment** reads a YAML or JSON map of names to values, a Postman environment export or a Bruno `.bru` environment.
- **Next to a test file.** Opening a test file also loads the files in the `environments/` folder beside it. `stormix import -o FILE` writes a collection's environments there, and `stormix run FILE --env NAME` finds them by name.

Environments stay in the list when you open another file or start from the example.

## Variables

**Environment › Variables** lists the chosen environment's variables to see and change.

<Shot name="env-variables" alt="The Variables dialog" />

Every change applies to runs at once, **for this session only**, and is marked *this session*:

- **Save** one variable, or **Save all to file**, to make the changes permanent.
- **Reset** one, or **Reset all**, to go back to the file.
- Add and remove variables, for the session or for good.

<Shot name="env-session" alt="Variables changed for this session" />

Saving writes the environment's own file in its own format: a `.bru` file keeps its disabled lines, a Postman export keeps its other fields, JSON and YAML are rewritten. An environment that came with a collection is saved to `environments/NAME.yaml` next to the test file, or asks where.

You can also change one value without opening the dialog, from its [hover](/app/variables-and-scripts#seeing-and-editing-a-value).

## Secrets

A secret is a variable whose value should never be written to disk: a password, an API key. Bruno's `vars:secret` are secrets, and a YAML environment lists its secret names under `$secrets`:

```yaml
baseUrl: https://staging.example.com
user: demo
$secrets: [password, apiKey]
```

**Environment › Secrets** asks for their values. They are kept in memory only: masked in the app, never saved to the environment file or the workspace, and asked for again after a restart. The Environment button turns amber while a secret is missing.

<Shot name="env-secrets" alt="An imported environment with secrets" />

On the command line, give secrets with `--var name=value`, ideally from your CI's secret store.
