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
  const { createJiti } = await import(
    "/Users/albertobellido/gs-root/shell/prefix/lib/node_modules/jiti/lib/jiti.mjs",
  );
  const jiti = createJiti(import.meta.url);
  const href = new URL(extensionPath, import.meta.url).href;
  return await jiti.import(href, { default: true });
}

const PANE = "w9:pZ";
const SOCKET = join("/tmp", `gs3-profile-${process.getuid()}-${PANE}.sock`);

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
const flags = new Map([["gs3-herdr-pane", PANE]]);
const setModelCalls = [];
const fakePi = {
  registerFlag: (name, options) => { flags.set(name, options?.default); },
  getFlag: (name) => flags.get(name),
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

  const ctx = {
    mode: "tui",
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
    && setModelCalls[0] === "provider/alpha-host", JSON.stringify(setModelCalls));

  reply = await request({ id: "3", method: "get", params: {} });
  check("get after use reports the binding", reply?.result?.profile?.name === "alpha", JSON.stringify(reply));

  reply = await request({ id: "4", method: "use", params: { name: "missing" } });
  check("use with an unknown name fails", reply?.result?.ok === false
    && Array.isArray(reply?.result?.names) && reply.result.names.includes("beta"), JSON.stringify(reply));
  check("failed use never touched the binding", globalThis[BINDINGS].get("sess-test-1")?.name === "alpha");

  reply = await request({ id: "5", method: "bogus", params: {} });
  check("unknown method is rejected", !!reply?.error, JSON.stringify(reply));

  await fire("session_shutdown", ctx);
  await new Promise((resolve) => setTimeout(resolve, 150));
  check("socket removed after session_shutdown", !existsSync(SOCKET));
} catch (error) {
  failed += 1;
  console.log(`FAIL harness crashed: ${error?.stack ?? error}`);
} finally {
  rmSync(fixture, { recursive: true, force: true });
}

console.log(`${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
