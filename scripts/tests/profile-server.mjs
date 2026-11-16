#!/usr/bin/env node
// Harness for .pi/extensions/profile-server.ts: a fake pi installs the
// extension, the socket serves get/use, and the session binding map records
// exactly what /gentle:profiles' "Use in this session" would record.
// Run: node scripts/tests/profile-server.mjs
import { createRequire } from "node:module";
import net from "node:net";
import { mkdtempSync, writeFileSync, rmSync, statSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const extensionPath = join(root, ".pi", "extensions", "profile-server.ts");

// jiti lives in the bundled prefix; a bare "jiti" import works when installed.
const require = createRequire(import.meta.url);
async function loadExtension() {
  // jiti ships with Pi; the harness resolves it without pinning one developer's home.
  const candidate = process.env.JITI_PATH ??
    ["/Users/albertobellido/gs-root/shell/prefix/lib/node_modules/jiti/lib/jiti.mjs"].find((p) => existsSync(p)) ??
    "jiti";
  const { createJiti } = await import(candidate);
  const jiti = createJiti(import.meta.url);
  const href = new URL(extensionPath, import.meta.url).href;
  return await jiti.import(href, { default: true });
}

const PANE = "w9:pZ";
// Kept short on purpose: unix socket paths cap near 104 bytes, and an isolated runner points this
// at its own short scratch directory through the same variable the extension reads.
const SOCKET_BASE = process.env.GS3_PROFILE_SOCKET_DIR || "/tmp";
process.env.GS3_PROFILE_SOCKET_DIR = SOCKET_BASE;
const socketOf = (pane) => join(SOCKET_BASE, `gs3-profile-${process.getuid()}-${pane}.sock`);
const SOCKET = socketOf(PANE);

function request(payload) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(SOCKET);
    let data = "";
    const timer = setTimeout(() => { socket.destroy(); reject(new Error("timeout")); }, 2000);
    socket.on("error", (error) => { clearTimeout(timer); reject(error); });
    socket.on("connect", () => socket.write(`${JSON.stringify(payload)}\n`));
    socket.on("data", (chunk) => {
      data += chunk;
      if (!data.includes("\n")) return;
      clearTimeout(timer);
      socket.destroy();
      try { resolve(JSON.parse(data.slice(0, data.indexOf("\n")))); }
      catch (error) { reject(error); }
    });
  });
}

let passed = 0;
let failed = 0;
function check(name, condition, detail) {
  if (condition) { passed += 1; console.log(`PASS ${name}`); }
  else { failed += 1; console.log(`FAIL ${name}${detail ? `: ${detail}` : ""}`); }
}

const fixture = mkdtempSync(join(tmpdir(), "gs3-profile-server.XXXXXX"));
process.env.GENTLE_PI_CONFIG_HOME = fixture;
writeFileSync(join(fixture, "profiles.json"), JSON.stringify({
  kind: "gentle-pi.agent_model_profiles",
  version: 1,
  active: "alpha",
  profiles: {
    alpha: { orchestrator: { model: "provider/alpha-host" }, explore: { model: "provider/alpha-explore" } },
    beta: { explore: { model: "provider/beta-explore" } },
  },
}));

// The shared binding store gentle-pi owns; the extension writes through it.
const BINDINGS = Symbol.for("gentle-pi:session-profile-bindings");
globalThis[BINDINGS] = new Map();

const handlers = {};
// The gs3 launcher passes the pane as a CLI argument before any "--" separator; the bridge
// owns the flag registration, so the extension reads the value straight from argv.
process.argv = ["node", "agent", "--gs3-herdr-pane", PANE];
const registered = new Set();
const commands = new Set();
const setModelCalls = [];
const fakePi = {
  // The extension registers a command and subscribes to events now that it also pushes to the rail.
  registerCommand: (name) => { commands.add(name); },
  events: { on: () => {} },
  registerFlag: (name) => { registered.add(name); },
  getFlag: () => undefined,
  on: (event, handler) => { (handlers[event] ??= []).push(handler); },
  setModel: async (model) => { setModelCalls.push(model); return true; },
};

async function fire(event, ctx) {
  for (const handler of handlers[event] ?? []) await handler({}, ctx);
}

try {
  const install = await loadExtension();
  check("extension default export is a function", typeof install === "function");
  install(fakePi);
  // The herdr-session bridge owns the gs3-herdr-pane flag; a second registration is a loader
  // conflict ("Flag \"--gs3-herdr-pane\" conflicts with ..."), so this extension registers
  // nothing and reads the launcher's argument from argv instead.
  check("extension registers no flags", registered.size === 0, [...registered].join(", "));
  check("extension registers the /gs3 command", commands.has("gs3"));

  const model = { provider: "provider", id: "alpha-host" };
  const ctx = {
    mode: "tui",
    modelRegistry: { find: (provider, id) => provider === model.provider && id === model.id ? model : undefined },
    sessionManager: { getSessionId: () => "sess-test-1", getSessionFile: () => "/tmp/sess-test-1.jsonl" },
  };
  await fire("session_start", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));

  check("socket exists after session_start", existsSync(SOCKET));
  const stat = statSync(SOCKET);
  check("socket is a socket", stat.isSocket?.() ?? stat.Socket ?? true, String(stat.mode));
  check("socket is 0600", (stat.mode & 0o777) === 0o600, (stat.mode & 0o777).toString(8));

  let reply = await request({ id: "1", method: "get", params: {} });
  check("get before use reports no binding", reply?.result?.profile === null, JSON.stringify(reply));

  reply = await request({ id: "2", method: "use", params: { name: "alpha" } });
  check("use alpha is ok", reply?.result?.ok === true, JSON.stringify(reply));
  check("use alpha returns its models", reply?.result?.profile?.name === "alpha"
    && reply?.result?.profile?.models?.orchestrator?.model === "provider/alpha-host", JSON.stringify(reply));

  const binding = globalThis[BINDINGS].get("sess-test-1");
  check("binding recorded for the session", binding?.name === "alpha" && binding?.origin === "user"
    && binding?.modelProfiles?.explore?.model === "provider/alpha-explore", JSON.stringify(binding));
  check("live model switched to the orchestrator route", setModelCalls.length === 1
    && setModelCalls[0] === model, JSON.stringify(setModelCalls));

  reply = await request({ id: "3", method: "get", params: {} });
  check("get after use reports the binding", reply?.result?.profile?.name === "alpha", JSON.stringify(reply));

  reply = await request({ id: "4", method: "use", params: { name: "missing" } });
  check("use with an unknown name fails", reply?.result?.ok === false
    && Array.isArray(reply?.result?.names) && reply.result.names.includes("beta"), JSON.stringify(reply));
  check("failed use never touched the binding", globalThis[BINDINGS].get("sess-test-1")?.name === "alpha");

  reply = await request({ id: "5", method: "bogus", params: {} });
  check("unknown method is rejected", !!reply?.error, JSON.stringify(reply));

  // A malformed line still gets an answer: JSON.parse runs before any promise exists.
  reply = await new Promise((resolve, reject) => {
    const socket = net.createConnection(SOCKET);
    const timer = setTimeout(() => { socket.destroy(); reject(new Error("timeout")); }, 2000);
    let data = "";
    socket.on("error", (error) => { clearTimeout(timer); reject(error); });
    socket.on("connect", () => socket.write("{not json}\n"));
    socket.on("data", (chunk) => {
      data += chunk;
      if (!data.includes("\n")) return;
      clearTimeout(timer); socket.destroy();
      try { resolve(JSON.parse(data.slice(0, data.indexOf("\n")))); } catch (error) { reject(error); }
    });
  });
  check("malformed request is answered", reply?.error?.message === "malformed request", JSON.stringify(reply));
  reply = await request({ id: "6", method: "get", params: {} });
  check("server still serves after a malformed line", reply?.result?.profile?.name === "alpha", JSON.stringify(reply));

  await fire("session_shutdown", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));
  check("socket removed after session_shutdown", !existsSync(SOCKET));

  // A pane id after the "--" separator is a user argument, never a binding: dormant.
  process.argv = ["node", "agent", "--", "--gs3-herdr-pane", "w9:pX"];
  await fire("session_start", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));
  check("dormant when the pane only appears after --", !existsSync(socketOf("w9:pX")));

  // The equals form is accepted too.
  process.argv = ["node", "agent", "--gs3-herdr-pane=w9:pY"];
  await fire("session_start", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));
  check("equals form starts the channel", existsSync(socketOf("w9:pY")));
  await fire("session_shutdown", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));
  check("equals-form socket removed after shutdown", !existsSync(socketOf("w9:pY")));

  // MAIN · Agente: the rail is told both ends of a turn. Only agent_start used to be forwarded, so
  // the last line kept claiming a turn was running long after it had finished.
  process.argv = ["node", "agent", "--gs3-herdr-pane", PANE];
  const agentDir = mkdtempSync("/tmp/gs3-agent-events.XXXXXX");
  process.env.GS3_AGENT_EVENT_DIR = agentDir;
  const pushed = [];
  const agentServer = net.createServer((socket) => {
    let buffer = "";
    socket.on("data", (chunk) => { buffer += chunk; });
    socket.on("close", () => {
      for (const line of buffer.split("\n")) { if (line.trim()) { try { pushed.push(JSON.parse(line)); } catch { /* a broken line is not an event */ } } }
    });
  });
  const agentSocket = join(agentDir, `gs3-agent-${process.getuid()}-${PANE}.sock`);
  await new Promise((resolve) => agentServer.listen(agentSocket, resolve));
  await fire("agent_start", ctx);
  await new Promise((resolve) => setTimeout(resolve, 250));
  await fire("agent_settled", ctx);
  await new Promise((resolve) => setTimeout(resolve, 400));
  agentServer.close();
  check("agent_start reports the turn running", pushed[0]?.type === "working" && pushed[0]?.text === "turno en marcha", JSON.stringify(pushed));
  check("agent_settled reports the turn finished", pushed[1]?.type === "idle" && pushed[1]?.text === "turno terminado", JSON.stringify(pushed));
  check("every pushed event carries the emitter's time", pushed.length === 2 && pushed.every((e) => Number.isFinite(e.ts) && e.ts > 0), JSON.stringify(pushed));
  rmSync(agentDir, { recursive: true, force: true });
} catch (error) {
  failed += 1;
  console.log(`FAIL harness crashed: ${error?.stack ?? error}`);
} finally {
  rmSync(fixture, { recursive: true, force: true });
}

console.log(`${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
