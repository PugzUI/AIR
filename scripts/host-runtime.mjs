import { randomBytes, randomUUID } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { lstat, mkdir, readFile, realpath, rename, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const DEMO_ROOT = join(ROOT, ".local", "demo");
export const DEMO_BOOTSTRAP = join(DEMO_ROOT, "bootstrap.json");
export const DEMO_TEMPLATE = join(ROOT, "deploy", "relay-demo@.service");
export const DEMO_HOSTS = Object.freeze([
  { id: "primary", port: 42000, unit: "relay-demo@primary.service", sessions: 15,
    providers: ["scout", "builder", "reviewer", "qa", "planner", "watcher"] },
  { id: "secondary", port: 42001, unit: "relay-demo@secondary.service", sessions: 9,
    providers: ["builder", "reviewer", "qa", "watcher"] },
]);
const SYSTEMD = ["--user"];

function run(binary, args, options = {}) {
  const result = spawnSync(binary, args, {
    encoding: "utf8", timeout: 15000, ...options,
  });
  if (result.error) throw Error(binary + ": " + result.error.message);
  if (result.status !== 0) {
    throw Error(binary + " " + args.join(" ") + ": " +
      (result.stderr || result.stdout || "failed").trim());
  }
  return result.stdout.trim();
}

export function systemctl(...args) {
  return run("systemctl", [...SYSTEMD, ...args]);
}

export function unitProperty(unit, property) {
  return systemctl("show", unit, "--property=" + property, "--value");
}

export async function assertUnit(unit, expected) {
  const fragment = unitProperty(unit, "FragmentPath");
  if (!fragment) throw Error(unit + " is not installed; refusing to manage an unknown service");
  if (await realpath(fragment) !== await realpath(expected)) {
    throw Error(unit + " belongs to a different unit (" + fragment + "); refusing to replace it");
  }
}

export async function ensureDemoUnits() {
  // A linked template is stable across invocations; systemd owns the complete
  // process group and reaps ACP children before starting the replacement.
  if (!unitProperty(DEMO_HOSTS[0].unit, "FragmentPath")) {
    systemctl("link", DEMO_TEMPLATE);
  }
  // Refresh the template if this checkout's unit changed since the last run.
  systemctl("daemon-reload");
  for (const host of DEMO_HOSTS) await assertUnit(host.unit, DEMO_TEMPLATE);
}

export async function ensureRelayConsole({ restart = false } = {}) {
  await assertUnit("relay.service", join(ROOT, "deploy", "relay.service"));
  systemctl("daemon-reload");
  assertPortOwnedOrFree(4200, "relay.service");
  if (unitProperty("relay.service", "ActiveState") !== "active") {
    systemctl("start", "relay.service");
  } else if (restart) {
    systemctl("restart", "relay.service");
  }
  await waitForOwnedPort(4200, "relay.service");
}

export function buildRelayConsole() {
  run(process.execPath, [join(ROOT, "scripts", "build.mjs")], { cwd: ROOT });
}

export function listeningPids(port) {
  const output = run("ss", ["-H", "-ltnp", "( sport = :" + port + " )"]);
  if (!output) return [];
  const listeners = output.split("\n");
  const pids = [];
  for (const line of listeners) {
    const matches = [...line.matchAll(/pid=(\d+)/g)].map((item) => Number(item[1]));
    if (!matches.length) throw Error("Port " + port + " is occupied but its process cannot be verified");
    pids.push(...matches);
  }
  return [...new Set(pids)];
}

export function mainPid(unit) {
  return Number(unitProperty(unit, "MainPID")) || 0;
}

export function assertPortOwnedOrFree(port, unit) {
  const pids = listeningPids(port);
  if (!pids.length) return;
  const pid = mainPid(unit);
  if (!pid || !pids.every((owner) => owner === pid)) {
    throw Error("Port " + port + " belongs to another process (" + pids.join(", ") + "). " +
      "Only " + unit + " may be stopped; no unrelated process was signalled.");
  }
}

export async function waitForOwnedPort(port, unit, timeoutMs = 20000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (unitProperty(unit, "ActiveState") === "failed") {
      throw Error(unit + " failed while starting");
    }
    const pid = mainPid(unit);
    const pids = listeningPids(port);
    if (pid && pids.length && pids.every((owner) => owner === pid) &&
        unitProperty(unit, "ActiveState") === "active") return pid;
    if (pids.length && (!pid || pids.some((owner) => owner !== pid))) {
      throw Error(unit + " does not own the listener on " + port);
    }
    await sleep(150);
  }
  throw Error(unit + " did not claim its expected listener on :" + port);
}

export async function atomicWrite(file, value, mode = 0o600) {
  await mkdir(dirname(file), { recursive: true, mode: 0o700 });
  const temp = file + "." + process.pid + "." + randomUUID() + ".tmp";
  try {
    await writeFile(temp, value, { mode, flag: "wx" });
    await rename(temp, file);
  } finally {
    await rm(temp, { force: true });
  }
}

export async function rotateToken(file) {
  if (!isAbsolute(file)) throw Error("Token file must be absolute: " + file);
  try {
    const info = await lstat(file);
    if (!info.isFile() || info.uid !== process.getuid() || (info.mode & 0o077)) {
      throw Error("Token file ownership or permissions are unsafe: " + file);
    }
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const previous = existsSync(file) ? await readFile(file) : null;
  const token = randomBytes(32).toString("hex");
  await atomicWrite(file, token + "\n");
  return { token, previous };
}

export async function restoreToken(file, contents) {
  if (contents === null) await rm(file, { force: true });
  else await atomicWrite(file, contents);
}

export function copyToken(token) {
  const choices = [];
  if (process.env.WAYLAND_DISPLAY) choices.push(["wl-copy", ["--sensitive", "--type", "text/plain"]]);
  if (process.env.DISPLAY) choices.push(["xclip", ["-selection", "clipboard"]]);
  if (process.env.WSL_DISTRO_NAME) choices.push(["clip.exe", []]);
  if (process.platform === "darwin") choices.push(["pbcopy", []]);
  for (const [binary, args] of choices) {
    try {
      // wl-copy forks a clipboard owner that intentionally stays alive. Its
      // inherited stderr must not hold execFileSync's pipe open indefinitely.
      execFileSync(binary, args, { input: token, stdio: ["pipe", "ignore", "ignore"], timeout: 4000 });
      return binary;
    } catch (error) {
      if (error.code === "ENOENT") continue;
    }
  }
  throw Error("No working clipboard backend found. The host is running; " +
    "its credential remains in its mode-0600 token file.");
}

export async function productionConfig() {
  const file = join(process.env.HOME, ".config", "ahpd", "config.json");
  const config = JSON.parse(await readFile(file, "utf8"));
  const info = await stat(file);
  if (info.uid !== process.getuid()) throw Error("Production config is not owned by this user: " + file);
  const port = Number(config.port ?? 9187);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw Error("Production port is invalid");
  }
  if (!config.connectionTokenFile || !isAbsolute(config.connectionTokenFile)) {
    throw Error("Production host requires an absolute connectionTokenFile");
  }
  const exec = unitProperty("ahpd.service", "ExecStart");
  if (!exec.includes("--config-file " + file)) {
    throw Error("ahpd.service does not use the expected production configuration");
  }
  await assertUnit("ahpd.service", join(process.env.HOME, ".config", "systemd", "user", "ahpd.service"));
  return { file, config, port, tokenPath: config.connectionTokenFile };
}
