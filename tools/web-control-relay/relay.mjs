#!/usr/bin/env node
// Loopback relay between the desktop control protocol and the browser build.
//
//   cadcraft-cli mcp --connect 127.0.0.1:7979   ──TCP, JSON lines──▶  this relay
//   browser: https://…/?control=ws://localhost:7980  ◀──WebSocket text frames──  this relay
//
// Every line from a TCP client is forwarded to the browser tab as one text frame; every frame
// from the browser is written back to the TCP client that sent the request (matched by `id`).
// A request that arrives with no tab connected is answered with an error line, so the MCP
// server reports it instead of hanging. Both listeners bind loopback only.
//
// Usage: node relay.mjs [--tcp 7979] [--ws 7980]

import net from "node:net";
import { WebSocketServer } from "ws";

const args = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? Number(args[i + 1]) : dflt;
};
const TCP_PORT = opt("--tcp", 7979);
const WS_PORT = opt("--ws", 7980);
const HOST = "127.0.0.1";

let tab = null; // the one browser tab being driven
const waiting = new Map(); // id (as JSON text) -> TCP socket that asked
let anon = 0; // requests sent without an id

const wss = new WebSocketServer({ host: HOST, port: WS_PORT });
wss.on("listening", () => console.log(`relay: browser  ws://${HOST}:${WS_PORT}`));
wss.on("connection", (ws, req) => {
  if (tab) {
    console.log("relay: a second tab connected; replacing the first");
    tab.close(1000, "replaced by a newer tab");
  }
  tab = ws;
  console.log(`relay: tab connected from ${req.socket.remoteAddress}`);
  ws.on("message", (data) => {
    for (const line of data.toString().split("\n")) {
      if (!line.trim()) continue;
      let id = null;
      try {
        id = JSON.stringify(JSON.parse(line).id ?? null);
      } catch {
        /* unparseable reply: deliver to whoever is waiting longest */
      }
      const client = (id !== null && waiting.get(id)) || waiting.values().next().value;
      if (id !== null) waiting.delete(id);
      if (client && !client.destroyed) client.write(line + "\n");
    }
  });
  ws.on("close", () => {
    if (tab === ws) tab = null;
    console.log("relay: tab disconnected");
    for (const [id, client] of waiting) {
      if (!client.destroyed) client.write(JSON.stringify({ id: JSON.parse(id), ok: false, error: "the browser tab disconnected", state: "may-have-run" }) + "\n");
    }
    waiting.clear();
  });
});

const tcp = net.createServer((socket) => {
  console.log("relay: control client connected");
  let buf = "";
  socket.on("data", (chunk) => {
    buf += chunk.toString();
    let nl;
    while ((nl = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, nl).replace(/\r$/, "");
      buf = buf.slice(nl + 1);
      if (!line.trim()) continue;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch (e) {
        socket.write(JSON.stringify({ ok: false, error: `bad JSON: ${e.message}` }) + "\n");
        continue;
      }
      if (msg.id === undefined) msg.id = `relay-${++anon}`;
      if (!tab || tab.readyState !== tab.OPEN) {
        socket.write(JSON.stringify({ id: msg.id, ok: false, error: `no browser tab connected: open the app with ?control=ws://localhost:${WS_PORT}`, state: "not-run" }) + "\n");
        continue;
      }
      waiting.set(JSON.stringify(msg.id), socket);
      tab.send(JSON.stringify(msg));
    }
  });
  socket.on("close", () => {
    for (const [id, client] of waiting) if (client === socket) waiting.delete(id);
    console.log("relay: control client disconnected");
  });
  socket.on("error", () => {});
});
tcp.listen(TCP_PORT, HOST, () => console.log(`relay: control  tcp://${HOST}:${TCP_PORT}  (cadcraft-cli mcp --connect ${HOST}:${TCP_PORT})`));
