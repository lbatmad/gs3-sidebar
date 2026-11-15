/**
 * gs3 profile channel — a direct HUD ↔ session channel for loading profiles.
 *
 * The gs3-hud rail derives this socket's path from its --pane argument
 * (/tmp/gs3-profile-<uid>-<pane>.sock) and sends no text or keys to the agent
 * pane: a profile click loads the profile through this socket instead of
 * typing a command into the chat. The pane id arrives as the gs3-herdr-pane
 * flag registered by the herdr-session bridge; without it there is no rail to
 * serve and the extension stays dormant.
 *
 * Protocol: one JSON request per connection, one JSON reply per line.
 *   {"id":..,"method":"get","params":{}}
 *       → {"result":{"profile":null | {name, models}}}   models: role → {model, tier?}
 *   {"id":..,"method":"use","params":{"name":".."}}
 *       → {"result":{"ok":true,"profile":{name, models}}}
 *       → {"result":{"ok":false,"error":"..","names":[..]}}
 *
 * "use" binds the profile to this session exactly like /gentle:profiles'
 * "Use in this session" action (u): it writes {name, origin: "user",
 * modelProfiles} into gentle-pi's process-wide session binding map
 * (Symbol.for("gentle-pi:session-profile-bindings"), the same store the
 * profile-use extension writes) and, when the profile has an orchestrator
 * model, switches the live model via pi.setModel().
 *
 * Nothing is written to disk: no repo declarations, no clone pins, no global
 * routing. Names travel as JSON data, never as command text.
 *
 * The socket base is fixed /tmp on purpose: the bundled shell rebuilds the
 * environment, so TMPDIR differs between the rail and the agent process.
 */
import net from "node:net";
import { readFileSync, unlinkSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const BINDINGS_SYMBOL_KEY = "gentle-pi:session-profile-bindings";
const SOCKET_BASE = "/tmp";

interface RoutingEntry {
	model?: string;
	tier?: string;
}
type ModelConfig = Record<string, RoutingEntry>;

function configHome(): string {
	return process.env.GENTLE_PI_CONFIG_HOME ?? join(homedir(), ".pi", "gentle-ai");
}

function readProfiles(): { names: string[]; profiles: Record<string, ModelConfig> } | undefined {
	let raw: string;
	try {
		raw = readFileSync(join(configHome(), "profiles.json"), "utf8");
	} catch {
		return undefined;
	}
	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return undefined;
	}
	const profiles = (parsed as { profiles?: unknown }).profiles;
	if (typeof profiles !== "object" || profiles === null) return undefined;
	return { names: Object.keys(profiles), profiles: profiles as Record<string, ModelConfig> };
}

function bindingsStore(): Map<string, unknown> | undefined {
	const holder = globalThis as unknown as Record<symbol, unknown>;
	const value = holder[Symbol.for(BINDINGS_SYMBOL_KEY)];
	return value instanceof Map ? (value as Map<string, unknown>) : undefined;
}

function socketPath(pane: string): string {
	return join(SOCKET_BASE, `gs3-profile-${process.getuid()}-${pane}.sock`);
}

function reply(socket: net.Socket, payload: Record<string, unknown>): void {
	socket.end(`${JSON.stringify(payload)}\n`);
}

export default function profileChannelExtension(pi: ExtensionAPI): void {
	let server: net.Server | undefined;
	let serving = "";
	let sessionId: string | undefined;
	let setModel: ((model: string) => Promise<boolean> | boolean) | undefined;

	function currentBinding(): { name: string; models: ModelConfig } | null {
		if (sessionId === undefined) return null;
		const bindings = bindingsStore();
		const entry = bindings?.get(sessionId) as { name?: unknown } | undefined;
		if (!entry || typeof entry.name !== "string") return null;
		const store = readProfiles();
		const models = store?.profiles[entry.name];
		if (!models) return null;
		return { name: entry.name, models };
	}

	function handle(request: unknown, socket: net.Socket): void {
		const id = (request as { id?: unknown })?.id;
		const method = (request as { method?: unknown })?.method;
		const params = (request as { params?: Record<string, unknown> })?.params ?? {};
		if (method === "get") {
			reply(socket, { id, result: { profile: currentBinding() } });
			return;
		}
		if (method === "use") {
			const store = readProfiles();
			if (!store) {
				reply(socket, { id, result: { ok: false, error: "no store" } });
				return;
			}
			const name = params.name;
			if (typeof name !== "string" || !Object.prototype.hasOwnProperty.call(store.profiles, name)) {
				reply(socket, { id, result: { ok: false, error: "not found", names: store.names } });
				return;
			}
			if (sessionId === undefined) {
				reply(socket, { id, result: { ok: false, error: "no session id" } });
				return;
			}
			const bindings = bindingsStore();
			if (!bindings) {
				reply(socket, { id, result: { ok: false, error: "binding store unavailable" } });
				return;
			}
			const models = store.profiles[name]!;
			bindings.set(sessionId, { name, origin: "user", modelProfiles: models });
			// A profile with an orchestrator route switches the live model, mirroring
			// the panel's live orchestrator switch.
			const orchestrator = models["orchestrator"];
			if (typeof orchestrator?.model === "string" && orchestrator.model.length > 0 && setModel) {
				try {
					void setModel(orchestrator.model);
				} catch {
					// The binding is already recorded; a failed live switch must not
					// fail the reply.
				}
			}
			reply(socket, { id, result: { ok: true, profile: { name, models } } });
			return;
		}
		reply(socket, { id, error: { message: "unknown method" } });
	}

	function stop(): void {
		const path = serving;
		serving = "";
		const running = server;
		server = undefined;
		running?.close();
		if (path) {
			try {
				unlinkSync(path);
			} catch {
				// Already gone.
			}
		}
	}

	function start(pane: string): void {
		if (serving === pane) return;
		stop();
		const path = socketPath(pane);
		// A previous session may have died without unlinking its socket.
		try {
			unlinkSync(path);
		} catch {
			// Nothing stale.
		}
		const listening = net.createServer((socket) => {
			socket.on("error", () => socket.destroy());
			let data = "";
			socket.on("data", (chunk) => {
				data += chunk;
				if (data.length > 65536) {
					socket.destroy();
					return;
				}
				const newline = data.indexOf("\n");
				if (newline === -1) return;
				const line = data.slice(0, newline);
				try {
					handle(JSON.parse(line), socket);
				} catch {
					reply(socket, { error: { message: "malformed request" } });
				}
			});
		});
		listening.on("error", () => {
			// Another listener (a second session on the same pane) wins; stay dormant.
			if (server === listening) stop();
		});
		listening.listen(path, () => {
			try {
				chmodSync(path, 0o600);
			} catch {
				// Best effort: the socket is still keyed by uid in its name.
			}
			server = listening;
			serving = pane;
		});
	}

	pi.on("session_start", (_event, ctx) => {
		// Headless modes (RPC/JSON/print) have no PTY and no rail beside them.
		if (ctx?.mode !== "tui") return;
		try {
			sessionId = ctx.sessionManager?.getSessionId?.() ?? undefined;
		} catch {
			sessionId = undefined;
		}
		setModel = (model: string) => pi.setModel(model);
		let pane: string | undefined;
		try {
			const flag = pi.getFlag("gs3-herdr-pane");
			pane = typeof flag === "string" && flag.length > 0 ? flag : undefined;
		} catch {
			pane = undefined;
		}
		if (pane === undefined) return;
		start(pane);
	});
	pi.on("session_shutdown", () => stop());
}
