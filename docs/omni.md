# Omni topics

Tiles can chart a governed Omni topic instead of rows the model types. It reads your Omni URL and API key from the macOS Keychain or the environment, calls your Omni instance over HTTPS, and sends the rows straight to the chart engine. Nothing goes anywhere but your Omni instance ([ADR-008](adr/ADR-008-data-sources.md), [ADR-005](adr/ADR-005-privacy-and-secrecy.md), [ADR-013](adr/ADR-013-data-stays-local.md)).

## What reaches the model

Your harness (Claude Code, Codex, OpenCode or Pi) sends every tool result to its model provider. For a chart drawn from Omni rows, Tiles' result holds:

- the title the model itself wrote, the form, the card id and its size in cells;
- the path of the saved PNG on this machine;
- a line saying the values stayed on this machine.

It holds no picture of the card and no row, label or number from Omni. The card is drawn from the saved file: by `tiles run`'s wrapper, the Claude Code mod or Pi's extension. The model can still see Omni metadata it asks for: topic names and labels from `tiles_omni_topics`, and field names and types from `tiles_omni_fields`. A refusal names the model's own highlights and column names, so a model can learn whether a value it guessed exists in a topic, one call at a time.

To let the values on an Omni card reach the model too, so it can talk about the numbers, start the harness with `TILES_OMNI_VALUES_TO_MODEL=1` in its environment. Do that only if your company's AI policy allows Omni data in that provider. Charts of rows the model typed itself always list their values.

## Approving Omni calls

With Omni configured, each harness prompts you before every Tiles tool call, whether you start it with `tiles run` or installed Tiles with `tiles install`. That stops a model steered by a README, an issue or a data label from pulling a topic without your say.

- The Tiles MCP server marks `tiles_chart`, `tiles_omni_topics` and `tiles_omni_fields` as neither read-only nor closed-world, so a client that honours those hints asks first.
- Claude Code: `tiles run` pre-approves no Tiles tool, and `tiles install claude` adds none.
- OpenCode: `tiles run` and `tiles install opencode` both set the three tools to `ask`.
- Codex: `tiles run` sets the server's tools to `prompt`. `tiles install codex` adds `default_tools_approval_mode = "prompt"` under `[mcp_servers.tiles]` in `~/.codex/config.toml` (`$CODEX_HOME/config.toml` if set) and changes nothing else in the file. If you already set that key to another value, `tiles install codex` stops without touching it, since `codex mcp add` would drop it.
- Pi has no tool approvals of its own, so the Tiles extension asks before a chart that fetches rows from Omni: it names the topics and offers Allow, Allow for this session, or Decline. A Pi session with no one to answer, in print or JSON mode, refuses the call. A new, resumed or forked session asks again.

`tiles run --allow-omni <harness>` pre-approves the three tools for that session, and in Pi skips the question. Without Omni, `tiles_chart` stays read-only and runs without a prompt, since it can only draw rows the model typed.

`tiles install` and `tiles setup` write these settings only when Omni is already configured, so configure Omni first; `tiles setup` names the `tiles omni set-key` command when it finds an Omni URL with no key. If you installed Tiles before, `tiles omni set-key` updates what Tiles installed: it sets the three tools to `ask` in the OpenCode `config.json` Tiles wrote, adds `default_tools_approval_mode = "prompt"` to Codex's tiles server, and prints one line per file it changed. It leaves alone an OpenCode file with settings of yours and any approval value you set yourself. With the environment route there is no set-key step: run `tiles install <harness>` again with the variables set. `tiles doctor` warns about an install that still runs a Tiles tool without asking and names the command that fixes it.

## What a model with a shell can read

The harness's tool approval is the control. Tiles keeps Omni values out of the tool result and keeps the fetched rows off the disk ([ADR-027](adr/ADR-027-omni-rows-stay-in-memory.md)), but each card's picture shows the values and is saved in your cache directory. A model with an unrestricted shell or file reader can read any file your user can, the card pictures included, and Claude Code's Read tool shows a PNG to the model as an image. `tiles run` keeps two harnesses' file and shell tools out of the cache ([ADR-028](adr/ADR-028-keep-the-model-out-of-the-cache.md)):

- Claude Code: `tiles run claude` passes a settings file that denies the model Read and Bash access to the cache directory. Claude Code applies Read rules to Grep, Glob and the shell commands it knows read files, such as `cat`, and applies deny rules in every permission mode, bypass included. A shell command that spells the path some other way gets past the Bash rule. If you run Claude Code's sandbox, you can add the cache to its `denyRead` yourself, at the cost of `tiles open` and `tiles cache clear` inside the session. Claude Code takes one `--settings`, so a `--settings <file-or-json>` of your own is merged in: Tiles' deny rules are added to its `permissions.deny`, the launch names only the merged file, and a value that cannot be read or parsed stops the launch with one line naming it.
- OpenCode: `tiles run opencode` denies the cache through OpenCode's `external_directory` permission, which its read, grep, glob and shell tools ask before touching a path outside the project. OpenCode skips that check for paths inside the directory it started in, so start it in a project, not your home directory. A one-word `external_directory` setting of your own is replaced by Tiles' rule for that session.
- Codex and Pi: `tiles run` passes them no rule for the cache, and Pi has no tool approvals, so a Pi model's shell reads it unasked.

Sessions started without `tiles run` get none of these rules. The cache keeps the newest 300 cards; `tiles cache clear` removes every card with its call and entrance frames, and prints how many cards went. Rows a running session fetched were never on disk and leave with that session. Call files written by a Tiles release before [ADR-027](adr/ADR-027-omni-rows-stay-in-memory.md) keep their rows until their card is pruned, so run `tiles cache clear` once after upgrading.

## Set the key

Use a personal access token: it sees only what your Omni user can see. An organization API key works too ([Omni's authentication docs](https://docs.omni.co/api/authentication.md)).

On a Mac, store the URL and key in the login Keychain with Tiles itself:

```sh
tiles omni set-key https://yourcompany.omniapp.co
```

It reads the key with echo off, so it stays out of your shell history, and stores the URL as `tiles.omni/url` and the key as `tiles.omni/api-key@yourcompany.omniapp.co`. Because `tiles` creates both items, only this `tiles` binary reads them without asking. Any other program, including `/usr/bin/security` run by a model's shell command, gets a macOS prompt first. Check it once:

```sh
security find-generic-password -s tiles.omni -a api-key@yourcompany.omniapp.co -w
```

macOS should ask before showing anything; choose **Deny**. If it prints the key, an older item made with `security add-generic-password` is still there: run `tiles omni set-key` again, which replaces both items. After you update `tiles`, macOS asks once whether the new binary may read the key; choose **Always Allow**.

The key is bound to its host: Tiles sends it only to the host stored with it, so a `url` item rewritten to another host finds no key.

Environment variables work as well, as a pair: `TILES_OMNI_URL` and `TILES_OMNI_API_KEY` are used only when both are set, and Tiles refuses to start Omni when just one is, rather than send a Keychain key to a URL from the environment. Prefer the Keychain on a machine with company data: every program the harness starts, including each shell command a model runs, inherits `TILES_OMNI_API_KEY` and can print it with `env`. Tiles never reads the key from a file, a command-line argument or the chart intent.

## Check it works

```sh
tiles omni topics
```

That lists every visible topic in your shared models. If it fails, the error names the Omni endpoint and the problem, such as `GET /v1/models answered 401; Omni refused the API key`. The error never holds your key or any row, so you can paste it into a bug report as is.

## Fetch rows for a chart

List a topic's fields, then write an intent whose `data` names the topic, the fields, the sort and the limit:

```sh
tiles omni fields orders
```

```json
{
  "data": {
    "omni": {
      "topic": "orders",
      "fields": ["orders.created_at[week]", "orders.count"],
      "sorts": [{ "field": "orders.created_at[week]", "desc": false }],
      "limit": 12
    }
  }
}
```

```sh
tiles omni rows intent.json
```

`rows` runs the query and prints only its shape: the row count and each column's type. A date dimension takes a timeframe in brackets, from the list `fields` prints next to it.

## What Tiles does and refuses

| Area | What Tiles does |
|---|---|
| Models | Every shared model, or only the one in `TILES_OMNI_MODEL_ID` (or Keychain account `model-id`). Set it when a topic name exists in two models |
| Filters | The topic's own default filters, no others (filters arrive in a later stage) |
| Rows | 5,000 by default, at most 10,000; text cells up to 1,000 characters |
| Time | 10 s to connect, 60 s for the whole call including retries and waiting on a slow query |
| Retries | Up to three attempts on a rate limit (Omni allows 60 requests a minute) or a gateway error |
| Network | HTTPS only, certificates checked against the macOS trust store, `HTTPS_PROXY` honoured, redirects never followed |
| Hostile text | A row value with a terminal escape or control character refuses the query; tabs and line breaks become spaces, and zero-width and other invisible format characters are dropped, except a joiner inside an emoji. Topics and fields whose names hold anything but plain identifier characters are left out of the listings |
