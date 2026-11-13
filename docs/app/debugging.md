# Debugging a failed request

When a step fails, Stormix tells you which check failed and shows you exactly what went over the wire. When the request never reaches the API, **Diagnose** finds out where it stopped.

## Details

Every step in **Last run**, and each of a load test's most common failures, has a **Details** button. It opens four tabs:

- **Failure** (or **Checks** when it passed): the error, the failed checks with what came back (such as `status is 200: got 401`), warnings, passed checks, the variables the step set, and its scripts' console output.
- **Request**: the headers and body as sent, after variables were filled in.
- **Response**: the status, headers and body as received, with JSON indented.
- **Connection**: the route the request took, and the proxy choice.

<Shot name="details" alt="Details of a failed step" single />

**Copy as cURL** copies the request as a `curl` command that sends the same thing, headers included, to try in a terminal or share. With [tracing](/features/tracing) on, the request's trace ID is there with **Copy trace ID**.

You don't need a run to get the command. Each step card in the **Flow** tab and each step in **Steps** has a **Copy as cURL** button that builds it from the step as it stands, with the values a run would send now: the flow's variables, the chosen environment with its session changes and secrets, the first row of any test data, and what earlier steps extracted in the last run of the same saved file with the same environment. A note under the button says:

- which shell it is quoted for: bash on macOS and Linux, Windows cmd on Windows, with the other one a click away;
- which `{{variables}}` had no value yet and stay as written, usually a token a login step sets: run the flow once and copy again;
- which secrets the command holds, since their real values are in it;
- that a pre-request script was not run, when the step has one, since what it changes is not in the command.

## The Connection tab

Every request records its route: the proxy it went through and where that setting came from, the address it connected to, the HTTP version and, without a proxy, the server's TLS certificate and issuer.

<Shot name="connection" alt="The Connection tab" />

The tab also has **Proxy for requests**, to switch between the system proxy, no proxy and a given proxy without leaving the dialog ([Proxies](/features/proxies)).

## Diagnose

**Diagnose** sends the step again, after the steps before it so its variables are set, and checks each stage on the way:

1. the proxy, and where its setting came from;
2. the name lookup;
3. the connection;
4. the proxy's answer when asked to reach the server;
5. the TLS certificate and who issued it;
6. the response.

An error is matched to whoever seems to have answered it: a corporate proxy, a firewall, a CDN or the API itself, from its headers and the title of an HTML error page. FortiGate and FortiProxy block pages are recognised. When a request failed through a proxy, Diagnose sends it once more without the proxy and says whether the proxy is what refuses it, with a **Use no proxy** button.

<Shot name="diagnose" alt="Diagnose shows each stage of a failing request" />

**Copy report** copies the whole diagnosis as text for a ticket. On the command line, the same check is `stormix diagnose FILE --step NAME` ([CLI](/reference/cli#diagnose)).

## Common causes

| You see | Likely cause | Try |
|---|---|---|
| 403 with an HTML page, the same request works in a browser or Bruno | A corporate proxy blocks the route | Diagnose, then **No proxy** or the right proxy |
| A certificate error | A self-signed or internal certificate | `--insecure` on the CLI for a test environment, or add the CA to the system |
| 401 on a later step | The token was not extracted or has expired | Check **Variables set** on the login step, and hover `{{token}}` |
| A wrong or empty value in the request | A `{{variable}}` that nothing defines | It is coloured as undefined; **Set** it from its hover or add it to the environment |
| A script error | A `pm.*` or `bru.*` call that is not supported | See [Compatibility](/reference/compatibility#scripts-and-tests) |
