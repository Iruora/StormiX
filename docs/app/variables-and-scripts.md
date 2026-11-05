# Variables and scripts

## Where a value comes from

A `{{name}}` in a URL, header or body is filled in when the request is sent. Values come from, the last one winning:

1. the flow's `variables`;
2. the chosen [environment](/app/environments);
3. values you typed for secrets, or set for this session;
4. values earlier steps **extracted** from their responses or that **scripts** set.

`{{$guid}}`, `{{$timestamp}}`, `{{$timestampMs}}` and `{{$randomInt}}` are generated fresh each time they are used.

The editors colour each variable by its source (defined, set at run time, generated, or undefined) and the YAML editor completes their names.

## Seeing and editing a value

Hover a `{{variable}}` in a Flow card or a Steps field. The card says its value and where it comes from: the flow, an environment, a secret, or the step that extracts it, with what the last run got.

<Shot name="var-hover" alt="Hovering a variable shows its value and an Edit button" />

Click **Edit** (or **Set**, when nothing defines it yet) to change it right there:

- **An environment variable** changes for this session at once and is marked as changed. **Save to file** writes it to the environment's file; **Reset** goes back to the file's value.
- **A secret** is kept in memory only, never written anywhere.
- **A flow variable** is changed in the test file.

<Shot name="var-edit" alt="Editing a variable's value from its hover" />

To see all the variables of an environment at once, open **Environment › Variables** ([Environments](/app/environments#variables)).

## Scripts

Each step can have a **Pre-request** script, run before the request is sent, and a **Post-response** script, run on the response. They are JavaScript in the style of Postman and Bruno, and they run in a sandboxed engine embedded in Stormix. Scripts imported from a collection, including collection and folder scripts, are there on each step.

Open them with the script button on a Flow card (it shows **pre**, **post** or a count when the step has scripts) or **scripts** in the Steps tab.

<Shot name="scripts-complete" alt="The script editor completing a res. call" />

The editor:

- **suggests** `bru`, `req`, `res`, `pm` and `expect(...)` members, your variable names inside `bru.getVar("…")` and `{{…}}`, and ready-made snippets for common tests and extractions;
- **underlines syntax errors** as you type;
- saves with Ctrl S.

<Shot name="scripts-error" alt="A syntax error underlined in a script" />

### What scripts can do

```js
// Post-response: check the response and keep a value for the next steps
test("returns the order", () => {
  expect(res.status).to.equal(201)
  expect(res.body.id).to.be.a("number")
})
bru.setVar("orderId", res.body.id)
```

```js
// Pre-request: fetch a token first, then use it
const login = await bru.sendRequest({
  method: "POST",
  url: bru.interpolate("{{baseUrl}}/login"),
  data: { user: "demo", password: bru.getEnvVar("password") },
})
req.setHeader("Authorization", `Bearer ${login.data.token}`)
```

- Scripts run as async functions, so `await` works at the top level.
- `bru.sendRequest` and `pm.sendRequest` send a request from a script; `bru.sleep(ms)` waits.
- Each script may run for 5 seconds (`--script-timeout` on the CLI). Time spent waiting on `sleep` and `sendRequest` does not count, up to 5 minutes.
- A failed `test` counts as a failed check, and in a load test as a failed request.

[Collections](/reference/collections) and [Compatibility](/reference/compatibility#scripts-and-tests) list which `pm.*` and `bru.*` calls are supported.
