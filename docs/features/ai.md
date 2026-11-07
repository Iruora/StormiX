# Explain results with AI

Stormix can ask the AI command line you already use, **Claude Code** or **GitHub Copilot CLI**, to explain a failed step or a load test. It runs that program on your computer with your own account, so Stormix needs no API key and opens no connection of its own: your company's Claude or Copilot plan, policy and proxy settings apply as they do in a terminal.

## Set it up

1. Install [Claude Code](https://docs.claude.com/en/docs/claude-code/setup) or [GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli).
2. Run `claude` or `copilot` once in a terminal and sign in.
3. Open **Settings → AI assistant**. Stormix lists what it found with its version. When both are there, pick one; you can also set a model (like `sonnet` or `gpt-5`), or the program's path when it is not on `PATH`, which is common for apps started from the Dock or the Start menu.

## Explain a failed step

In a step's **Details**, **Explain** (on the Failure tab, or the Explain tab) asks why the step failed. The AI sees the step as written in the test file, the request and response, failed checks, script logs, the connection, and the **Diagnose** result when you ran it. It answers with the most likely cause, the evidence and what to try next.

## Explain a load test

**Explain** at the top of a load test's results asks what the figures say: the summary, thresholds, each step's latency and errors, the timeline (thinned to about 40 seconds), the most common failures with the first failing response of each step, and the slowest traced requests.

## What is sent, and what is not

- Secrets are hidden before anything leaves Stormix: the values of the environment's `$secrets`, any variable named like a credential (`password`, `token`, `api_key`, `secret`…), `Authorization`, `Cookie` and similar headers (their scheme, like `Bearer`, is kept), bearer tokens, JWTs, `password=…`-style values and passwords in URLs. Each becomes `‹hidden›`.
- Bodies are cut to their first 3,000 and last 800 characters; a load test sends only the first 600 of each failing sample.
- Extracted values are not sent, only their names.
- **Show what is sent** displays the exact text the AI receives.
- The AI gets no tools. Claude Code runs with `--tools ""` and no MCP servers; Copilot CLI may only read the one file Stormix writes for it in an empty temporary folder, with shell, write, URL and memory tools denied. Both run outside your project, so they read none of your files or project instructions.
- Nothing is sent until you click **Explain**. Turn the buttons off in **Settings → AI assistant**.

## From the command line

`stormix explain` does the same with a saved result; see [`explain` in the CLI reference](/reference/cli#explain).

```sh
stormix run orders.stormix.yaml --format json > run.json
stormix explain run.json --file orders.stormix.yaml --secret "$API_PASSWORD"
```
