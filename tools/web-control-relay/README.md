# Driving the web build from an agent

The desktop app opens a control port (`cadcraft --control 7979`) that `cadcraft-cli mcp --connect`
wraps for AI agents. A browser can't open a port, so the web build does the reverse: with
`?control=ws://localhost:PORT` it connects *out* to this relay, which exposes the same JSON-lines
TCP protocol on loopback. Nothing in the MCP server changes; it thinks it is talking to the app.

```
agent ──MCP──▶ cadcraft-cli mcp --connect 127.0.0.1:7979
                          │ TCP, one JSON line per request
                   node relay.mjs            (this directory)
                          │ WebSocket, ws://localhost:7980
                   the web build in a tab    ?control=ws://localhost:7980
```

## Run it

```sh
# 1. the relay (needs Node 18+; `npm install` once for the ws package)
cd tools/web-control-relay && npm install && node relay.mjs

# 2. the web build, with the control flag — a local trunk serve or a deployed copy both work
#    (browsers allow an https page to open ws://localhost)
open 'http://127.0.0.1:8771/?control=ws://localhost:7980'

# 3. the MCP server, bridged to the relay exactly as it would be to the desktop app
cadcraft-cli mcp --connect 127.0.0.1:7979
```

Claude Code: `claude mcp add cadcraft -- cadcraft-cli mcp --connect 127.0.0.1:7979`.

## What works over the socket

Every method in `docs/control-protocol.md` that the web build can honour: `cmdline.*`,
`engine.execute`, `engine.commands`, `drawing.inspect`, `ui.inspect`, `ui.elements`, `ui.set`,
`ui.pointer`, `ui.render` (headless PNG, returned as base64; no `path` on the web) and `app.save`
(delivered as a browser download, or to the embedding page under `?host=parent`). `ui.screenshot`
and `app.open {path}` need a file system and are not available in a browser.

## Safety

- The page only accepts loopback targets (`localhost`, `127.0.0.1`, `[::1]`); a URL naming any
  other host is refused before a socket is opened (`apps/cadcraft-web/src/control_target.rs`).
- The relay binds `127.0.0.1` on both ports; nothing off the machine can reach it.
- One tab is driven at a time; a newer tab replaces the older one.
- A request that arrives while no tab is connected is answered with
  `{"ok": false, "error": "no browser tab connected…", "state": "not-run"}`, so an agent sees the
  problem rather than waiting on a 60 s timeout.
