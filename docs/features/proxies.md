# Proxies

By default, Stormix sends requests through the same proxy as the rest of your system, as browsers and Bruno do. You can also send them straight to the server, or through a proxy you name.

## Choosing the proxy

| Mode | Requests go |
|---|---|
| **System proxy** (default) | Through `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` if set, honouring `NO_PROXY`; otherwise through the proxy in the Windows or macOS settings, with its bypass list |
| **No proxy** | Straight to the server, whatever the variables and settings say, like Bruno with its proxy off |
| **Proxy** | Through the one proxy you give, such as `http://proxy.example:8080` |

Change it from the **proxy button** in the header, which always shows the mode in use, from **Settings › Proxy for requests**, or from a step's **Details › Connection**. The app keeps the choice across launches. On the command line, use `--proxy system|none|URL`.

<Shot name="proxy-refuses" alt="Diagnose showing a corporate proxy refusing the request" />

## When a corporate network gets in the way

A request that gets a **403** with an HTML page from Stormix, although the same request works in Bruno or a browser, is often refused by a corporate proxy or firewall rather than by your API. Open the step's **Details** and click **Diagnose**: it says who answered, and, when a proxy is involved, tries the request again without it. If the request gets through without it, **Use no proxy** switches the mode. [Debugging a failed request](/app/debugging#diagnose) explains every stage.

## Limits

- A proxy set only by an automatic configuration script (a PAC file, "Use setup script" on Windows) is not read. Set `HTTPS_PROXY` to the proxy that script picks for your API.
- Windows integrated sign-in to a proxy (NTLM or Kerberos) is not supported. A proxy that needs a user name and password takes them in its URL: `http://user:password@proxy.example.com:8080`.

See [Proxies in the CLI reference](/reference/cli#proxies).
