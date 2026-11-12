import { randomUUID } from "node:crypto";
import { mkdir, rm } from "node:fs/promises";
import { join, dirname } from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { MultiHostClient } from "@microsoft/agent-host-protocol/hosts";
import { WebSocketTransport } from "@microsoft/agent-host-protocol/ws";
import {
  ROOT, DEMO_ROOT, DEMO_BOOTSTRAP, DEMO_HOSTS, atomicWrite, assertPortOwnedOrFree, buildRelayConsole, copyToken,
  ensureDemoUnits, ensureRelayConsole, productionConfig, restoreToken, rotateToken, systemctl,
  waitForOwnedPort,
} from "./host-runtime.mjs";

const require = createRequire(import.meta.url);
const acpPlugin = dirname(dirname(require.resolve("@ahpd/agent-acp")));
const acpFixture = join(ROOT, "test", "fixtures", "fake-acp.mjs");
const kinds = {
  scout: "Scout", builder: "Builder", reviewer: "Reviewer",
  qa: "QA", planner: "Planner", watcher: "Watcher",
};
const captions = [
  "Startup verification", "Long-running graph layout", "Approval review", "Queue throughput",
  "Permission boundaries", "Session recovery", "Network reconnect", "Resource inspection",
  "Changeset review", "Multi-agent collaboration", "Dense transcript", "Agent selection",
  "Workspace identity", "Event subscriptions", "Cross-host isolation",
];
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function demoConfig(host, tokenPath) {
  const workspace = join(DEMO_ROOT, host.id, "workspace");
  return {
    host: "127.0.0.1",
    port: host.port,
    paths: [workspace],
    connectionTokenFile: tokenPath,
    origins: ["http://127.0.0.1:4200", "http://localhost:4200"],
    automations: "memory",
    sessions: "memory",
    updateCheck: false,
    plugins: [{
      name: acpPlugin,
      options: {
        presets: Object.fromEntries(host.providers.map((name) => [name, {
          name: kinds[name],
          command: process.execPath,
          args: [acpFixture],
        }])),
      },
    }],
  };
}

async function waitForHost(multi, host, token) {
  const url = new URL("ws://127.0.0.1:" + host.port);
  url.searchParams.set("tkn", token);
  await multi.addHost({
    id: host.id,
    label: host.id === "primary" ? "Demo Primary" : "Demo Secondary",
    transportFactory: () => WebSocketTransport.connect(url),
  });
  const until = Date.now() + 15000;
  while (Date.now() < until) {
    const state = multi.host(host.id);
    if (state?.state.status === "failed") {
      throw Error("Demo handshake failed on " + host.port);
    }
    if (state?.state.status === "connected" &&
        host.providers.every((provider) => state.agents.some((agent) => agent.provider === provider))) {
      return;
    }
    await sleep(100);
  }
  throw Error("Demo host on :" + host.port + " did not advertise all ACP fixtures");
}

async function seedHost(multi, host) {
  const id = host.id;
  const workspace = join(DEMO_ROOT, id, "workspace");
  const client = multi.client(id);
  multi.dispatch(id, "ahp-root://", {
    type: "root/configChanged",
    config: { workspaceTrust: {
      enabled: true,
      trustedUris: [pathToFileURL(workspace).href],
    } },
  });

  const seeded = [];
  for (let index = 0; index < host.sessions; index++) {
    const provider = host.providers[index % host.providers.length];
    const title = captions[index % captions.length];
    const uri = provider + ":/" + randomUUID();
    await client.request("createSession", {
      channel: uri, provider, workingDirectories: [pathToFileURL(workspace).href],
    });
    multi.dispatch(id, uri, { type: "session/titleChanged", title });
    seeded.push({ uri, title });
  }

  if (id === "primary") {
    // A fixed attention case and a genuine in-progress ACP turn. These use
    // the host's normal queue/approval pipeline, not synthetic UI flags.
    const samples = [
      [seeded[2], "Check approval once."],
      [seeded[1], "[demo:hold] Keep this turn active for the topology."],
      [seeded[4], "[demo:hold] Coordinating subagent execution plan."],
    ];
    for (const [item, prompt] of samples) {
      const session = await multi.subscribe(id, item.uri);
      const chat = session.snapshot?.state?.defaultChat;
      if (!chat) throw Error("Demo session has no host-advertised chat: " + item.title);
      await multi.subscribe(id, chat);
      multi.dispatch(id, chat, {
        type: "chat/pendingMessageSet", kind: "queued", id: randomUUID(),
        message: { text: prompt, origin: { kind: "user" } },
      });
    }
    // A transcript with multiple completed turns exercises the right drawer.
    const sample = seeded[10];
    const session = await multi.subscribe(id, sample.uri);
    const chat = session.snapshot?.state?.defaultChat;
    if (!chat) throw Error("Transcript demo session has no chat");
    await multi.subscribe(id, chat);
    for (let index = 1; index <= 9; index++) {
      multi.dispatch(id, chat, {
        type: "chat/pendingMessageSet", kind: "queued", id: randomUUID(),
        message: { text: "Transcript fixture step " + index, origin: { kind: "user" } },
      });
    }
  } else if (id === "secondary") {
    // Active working agents and attention items on the secondary host.
    const samples = [
      [seeded[0], "[demo:hold] Compiling changesets and lint checks."],
      [seeded[2], "[demo:hold] Running integration test sweep."],
      [seeded[6], "Check approval once."],
    ];
    for (const [item, prompt] of samples) {
      const session = await multi.subscribe(id, item.uri);
      const chat = session.snapshot?.state?.defaultChat;
      if (!chat) throw Error("Demo session has no host-advertised chat: " + item.title);
      await multi.subscribe(id, chat);
      multi.dispatch(id, chat, {
        type: "chat/pendingMessageSet", kind: "queued", id: randomUUID(),
        message: { text: prompt, origin: { kind: "user" } },
      });
    }
    const sample = seeded[4];
    const session = await multi.subscribe(id, sample.uri);
    const chat = session.snapshot?.state?.defaultChat;
    if (!chat) throw Error("Transcript demo session has no chat");
    await multi.subscribe(id, chat);
    for (let index = 1; index <= 5; index++) {
      multi.dispatch(id, chat, {
        type: "chat/pendingMessageSet", kind: "queued", id: randomUUID(),
        message: { text: "Secondary verification step " + index, origin: { kind: "user" } },
      });
    }
  }
  const catalog = await client.request("listSessions", { channel: "ahp-root://" });
  if (catalog.items.filter((item) => seeded.some((entry) => entry.uri === item.resource)).length !== seeded.length) {
    throw Error("Demo host did not publish every session in its catalog");
  }
  return seeded.length;
}

export async function startDemo({ announce = true, copyAccessToken = true } = {}) {
  await ensureDemoUnits();
  for (const host of DEMO_HOSTS) assertPortOwnedOrFree(host.port, host.unit);
  buildRelayConsole();
  await ensureRelayConsole({ restart: true });

  // Stop only verified, systemd-owned processes. This also reaps the old ACP
  // children and invalidates their in-memory demo sessions.
  for (const host of DEMO_HOSTS) systemctl("stop", host.unit);
  await rm(DEMO_BOOTSTRAP, { force: true });
  const tokenPath = join(DEMO_ROOT, "connection-token");
  const { token } = await rotateToken(tokenPath);
  for (const host of DEMO_HOSTS) {
    const workspace = join(DEMO_ROOT, host.id, "workspace");
    await mkdir(workspace, { recursive: true });
    await atomicWrite(join(DEMO_ROOT, host.id, "config.json"),
      JSON.stringify(demoConfig(host, tokenPath), null, 2) + "\n");
  }
  for (const host of DEMO_HOSTS) {
    systemctl("start", host.unit);
    await waitForOwnedPort(host.port, host.unit);
  }

  const multi = new MultiHostClient();
  const counts = [];
  try {
    for (const host of DEMO_HOSTS) {
      await waitForHost(multi, host, token);
      counts.push(await seedHost(multi, host));
    }
  } finally {
    await multi.shutdown();
  }
  await atomicWrite(DEMO_BOOTSTRAP, JSON.stringify({
    schemaVersion: 1,
    hosts: DEMO_HOSTS.map((host) => ({
      label: host.id === "primary" ? "Demo Primary" : "Demo Secondary",
      endpoint: "ws://127.0.0.1:" + host.port,
    })),
  }) + "\n");
  const clipboard = copyAccessToken ? copyToken(token) : null;
  if (announce) {
    console.log("Relay demo running: " + DEMO_HOSTS.map((h) =>
      "ws://127.0.0.1:" + h.port).join(" · "));
    console.log("Populated " + counts.reduce((sum, count) => sum + count, 0) +
      " sessions across two real AHP hosts and " +
      DEMO_HOSTS.reduce((sum, host) => sum + host.providers.length, 0) + " ACP fixtures.");
    if (clipboard) console.log("Fresh demo access token copied using " + clipboard + ". Token file: " + tokenPath);
    console.log("Open http://127.0.0.1:4200/; demo hosts bootstrap automatically on loopback.");
  }
  return { token, counts };
}

export async function stopDemo() {
  await ensureDemoUnits();
  for (const host of DEMO_HOSTS) {
    assertPortOwnedOrFree(host.port, host.unit);
    systemctl("stop", host.unit);
  }
  await rm(DEMO_BOOTSTRAP, { force: true });
  console.log("Stopped Relay demo units; production hosts untouched.");
}

export async function startDev() {
  const { port, tokenPath } = await productionConfig();
  assertPortOwnedOrFree(port, "ahpd.service");
  buildRelayConsole();
  await ensureRelayConsole();
  let demoStarted = false;
  let productionStopped = false;
  let previous = null;
  let token = null;
  // Only ahpd.service is allowed to stop/restart this production host. The
  // existing user config and persisted sessions remain intact.
  try {
    demoStarted = true;
    await startDemo({ announce: false, copyAccessToken: false });
    systemctl("stop", "ahpd.service");
    productionStopped = true;
    ({ token, previous } = await rotateToken(tokenPath));
    systemctl("start", "ahpd.service");
    await waitForOwnedPort(port, "ahpd.service");
  } catch (error) {
    if (productionStopped) {
      // Roll back the credential and try to restore the known previous host.
      if (previous) await restoreToken(tokenPath, previous);
      try { systemctl("restart", "ahpd.service"); }
      catch { /* Preserve the original failure as the primary diagnostic. */ }
    }
    if (demoStarted) await stopDemo();
    throw error;
  }
  const clipboard = copyToken(token);
  console.log("Production AHP host running: ws://127.0.0.1:" + port);
  console.log("Relay console: http://127.0.0.1:4200/");
  console.log("Rotated auth token copied using " + clipboard + ". Token file: " + tokenPath);
  console.log("Existing browser connections must reconnect with the new token.");
}

async function main(args) {
  const [mode, flag] = args;
  if (mode === "demo" && flag === "--stop") await stopDemo();
  else if (mode === "demo" && !flag) await startDemo();
  else if (mode === "dev" && !flag) await startDev();
  else throw Error("Usage: npm run demo [-- --stop] | npm run dev");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error("Relay host launch failed: " + error.message);
    process.exitCode = 1;
  });
}
