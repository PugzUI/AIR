import { createServer } from "node:http";
import { isIP } from "node:net";
import { spawn } from "node:child_process";
import { watch } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const base = resolve(process.env.RELAY_BUILD_DIR || join(root, "dist"));
// Demo host bootstrap is enabled only by the managed local service. Keeping
// the route opt-in prevents fixture and ad-hoc servers from reading the
// checkout's ignored demo credentials.
const demoBootstrap = process.env.RELAY_DEMO_BOOTSTRAP_FILE || "";
const demoToken = process.env.RELAY_DEMO_TOKEN_FILE || "";
const routes = new Map([
  ["/", ["index.html", "text/html; charset=utf-8"]],
  ["/app.js", ["app.js", "text/javascript; charset=utf-8"]],
  ["/style.css", ["style.css", "text/css; charset=utf-8"]],
]);
const port = Number(process.env.PORT || 4200);
const host = process.env.HOST || "127.0.0.1";
const previewEmbed = process.env.RELAY_PREVIEW === "1";
const contentSecurityPolicy = previewEmbed
  ? "default-src 'none'; script-src 'self' https://grok.com; style-src 'self'; connect-src 'self' ws: wss: https://grok.com; img-src 'self' data: https:; base-uri 'none'; frame-ancestors *; form-action 'none'"
  : "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw Error("PORT must be 1–65535");
}
if (!isIP(host)) {
  throw Error("HOST must be a literal IPv4 or IPv6 address");
}
const isLoopback = /** @param {any} address */ (address) => address === "::1" || address === "127.0.0.1" ||
  address === "::ffff:127.0.0.1";

const reloadClients = new Set();
if (demoBootstrap && demoToken) {
  /** @type {ReturnType<typeof setTimeout> | null} */
  let buildTimer = null;
  let building = false;
  let rebuildQueued = false;
  const runBuild = () => {
    if (building) { rebuildQueued = true; return; }
    building = true;
    const child = spawn(process.execPath, [join(root, "scripts", "build.mjs")], {
      cwd: root,
      env: { ...process.env, RELAY_BUILD_DIR: base },
      stdio: "ignore",
    });
    child.on("close", () => {
      building = false;
      if (rebuildQueued) {
        rebuildQueued = false;
        runBuild();
      }
    });
  };
  for (const dir of ["ui", "src"]) {
    try {
      watch(join(root, dir), { persistent: false }, (_event, filename) => {
        if (!filename || filename.startsWith(".")) return;
        clearTimeout(/** @type {ReturnType<typeof setTimeout>} */ (buildTimer));
        buildTimer = setTimeout(runBuild, 50);
      });
    } catch {}
  }
  /** @type {ReturnType<typeof setTimeout> | null} */
  let reloadTimer = null;
  const changed = new Set();
  try {
    watch(base, { persistent: false }, (_event, filename) => {
      if (!filename || filename.startsWith(".")) return;
      changed.add(filename);
      clearTimeout(/** @type {ReturnType<typeof setTimeout>} */ (reloadTimer));
      reloadTimer = setTimeout(() => {
        const kind = changed.size === 1 && changed.has("style.css") ? "css" : "reload";
        changed.clear();
        for (const client of reloadClients) client.write(`data: ${kind}\n\n`);
      }, 40);
    });
  } catch {}
}

const server = createServer(async (request, response) => {
  const pathname = (request.url || "/").split("?")[0];
  if (pathname === "/dev-reload") {
    if (request.method !== "GET") {
      response.writeHead(405, { Allow: "GET" }).end();
      return;
    }
    if (!demoBootstrap || !demoToken || !isLoopback(request.socket.remoteAddress)) {
      if (previewEmbed && !demoBootstrap && !demoToken) {
        response.writeHead(200, {
          "Content-Type": "text/event-stream; charset=utf-8",
          "Cache-Control": "no-store",
          Connection: "keep-alive",
          "X-Content-Type-Options": "nosniff",
        });
        response.write(": connected\n\n");
        return;
      }
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": "text/event-stream; charset=utf-8",
      "Cache-Control": "no-store",
      Connection: "keep-alive",
      "X-Content-Type-Options": "nosniff",
    });
    response.write(": connected\n\n");
    reloadClients.add(response);
    request.on("close", () => reloadClients.delete(response));
    return;
  }
  if (pathname === "/dev-hosts.json") {
    if (request.method !== "GET" && request.method !== "HEAD") {
      response.writeHead(405, { Allow: "GET, HEAD" }).end();
      return;
    }
    if (!demoBootstrap || !demoToken || !isLoopback(request.socket.remoteAddress)) {
      if (previewEmbed && !demoBootstrap && !demoToken) {
        const body = Buffer.from(JSON.stringify({ schemaVersion: 1, hosts: [] }) + "\n");
        response.writeHead(200, {
          "Content-Type": "application/json; charset=utf-8",
          "Content-Length": body.length,
          "Cache-Control": "no-store",
          "X-Content-Type-Options": "nosniff",
        });
        response.end(request.method === "HEAD" ? undefined : body);
        return;
      }
      response.writeHead(404).end();
      return;
    }
    try {
      const [config, token] = await Promise.all([
        readFile(demoBootstrap, "utf8"),
        readFile(demoToken, "utf8"),
      ]);
      const payload = JSON.parse(config);
      if (payload?.schemaVersion !== 1 || !Array.isArray(payload.hosts) ||
          !payload.hosts.every(/** @param {any} entry */ (entry) => typeof entry?.label === "string" &&
            typeof entry.endpoint === "string")) throw Error("invalid demo bootstrap");
      const body = Buffer.from(JSON.stringify({
        schemaVersion: 1,
        hosts: payload.hosts.map(/** @param {any} entry */ (entry) => ({
          label: entry.label,
          endpoint: entry.endpoint,
          token: token.trim(),
        })),
      }) + "\n");
      response.writeHead(200, {
        "Content-Type": "application/json; charset=utf-8",
        "Content-Length": body.length,
        "Cache-Control": "no-store",
        "X-Content-Type-Options": "nosniff",
      });
      response.end(request.method === "HEAD" ? undefined : body);
    } catch {
      response.writeHead(404).end();
    }
    return;
  }
  const route = routes.get(pathname);
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.writeHead(405, { Allow: "GET, HEAD" }).end();
    return;
  }
  if (!route) {
    response.writeHead(404).end();
    return;
  }
  try {
    const body = await readFile(join(base, route[0]));
    response.writeHead(200, {
      "Content-Type": route[1],
      "Content-Length": body.length,
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
      "Referrer-Policy": "no-referrer",
      "Content-Security-Policy": contentSecurityPolicy,
    });
    response.end(request.method === "HEAD" ? undefined : body);
  } catch {
    response.writeHead(503).end("Build the console with npm run console:build");
  }
});
server.listen(port, host, () => {
  console.log("Relay console listening on " + host + ":" + port);
});
