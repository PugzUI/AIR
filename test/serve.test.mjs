import test from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { once } from "node:events";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { isolatedFleet } from "./fixtures/isolated-hosts.mjs";

test("preview responses and relaxed CSP require RELAY_PREVIEW=1", { timeout: 15000 }, async () => {
  const root = await mkdtemp(join(tmpdir(), "relay-preview-"));
  await writeFile(join(root, "index.html"), "preview");
  try {
    for (const mode of ["", "true", "1"]) {
      const probe = createServer();
      probe.listen(0, "127.0.0.1");
      await once(probe, "listening");
      const port = probe.address().port;
      await new Promise((resolve, reject) => probe.close((error) => error ? reject(error) : resolve()));
      const child = spawn(process.execPath, ["src/serve.mjs"], {
        env: { ...process.env, HOST: "127.0.0.1", PORT: String(port), RELAY_BUILD_DIR: root,
          RELAY_PREVIEW: mode, RELAY_DEMO_BOOTSTRAP_FILE: "", RELAY_DEMO_TOKEN_FILE: "" },
        stdio: "ignore",
      });
      try {
        const origin = `http://127.0.0.1:${port}`;
        let response;
        for (let attempt = 0; attempt < 60; attempt++) {
          try {
            response = await fetch(origin);
            break;
          } catch {
            await new Promise((resolve) => setTimeout(resolve, 50));
          }
        }
        assert.equal(response?.status, 200);
        const csp = response.headers.get("content-security-policy");
        assert.match(csp, mode === "1" ? /frame-ancestors \*/ : /frame-ancestors 'none'/);
        assert.equal(csp.includes("https://grok.com"), mode === "1");
        const hosts = await fetch(origin + "/dev-hosts.json");
        assert.equal(hosts.status, mode === "1" ? 200 : 404);
        if (mode === "1") {
          assert.deepEqual(await hosts.json(), { schemaVersion: 1, hosts: [] });
          const head = await fetch(origin + "/dev-hosts.json", { method: "HEAD" });
          assert.equal(head.status, 200);
          assert.equal(await head.text(), "");
        }
        const reload = await fetch(origin + "/dev-reload", { signal: AbortSignal.timeout(1000) });
        assert.equal(reload.status, mode === "1" ? 200 : 404);
        if (mode === "1") {
          assert.match(reload.headers.get("content-type"), /^text\/event-stream/);
          await reload.body.cancel();
        }
      } finally {
        child.kill();
        if (child.exitCode === null) await once(child, "exit");
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("release server serves only local assets with strict headers and no mutations",
  { timeout: 30000 }, async (t) => {
    const { webOrigin } = await isolatedFleet(t, { withWeb: true });
    const response = await fetch(webOrigin);
    assert.equal(response.status, 200);
    assert.match(response.headers.get("content-type"), /^text\/html/);
    assert.equal(response.headers.get("cache-control"), "no-store");
    assert.equal(response.headers.get("x-content-type-options"), "nosniff");
    assert.equal(response.headers.get("referrer-policy"), "no-referrer");
    const csp = response.headers.get("content-security-policy");
    assert.match(csp, /default-src 'none'/);
    assert.match(csp, /script-src 'self'/);
    assert.match(csp, /frame-ancestors 'none'/);
    assert.doesNotMatch(csp, /unsafe-inline|unsafe-eval/);
    assert.match(await response.text(), /<relay-fleet>/);

    for (const name of ["app.js", "style.css"]) {
      const asset = await fetch(webOrigin + "/" + name);
      assert.equal(asset.status, 200);
      assert.ok((await asset.text()).length > 500);
      const head = await fetch(webOrigin + "/" + name, { method: "HEAD" });
      assert.equal(head.status, 200);
      assert.equal(head.headers.get("content-length"), asset.headers.get("content-length"));
      assert.equal(await head.text(), "");
    }
    const rejected = await fetch(webOrigin + "/app.js", { method: "POST" });
    assert.equal(rejected.status, 405);
    assert.equal(rejected.headers.get("allow"), "GET, HEAD");
    assert.equal((await fetch(webOrigin + "/../../.env")).status, 404);
    assert.equal((await fetch(webOrigin + "/does-not-exist")).status, 404);
    assert.equal((await fetch(webOrigin + "/dev-hosts.json")).status, 404);
    assert.equal((await fetch(webOrigin + "/dev-reload")).status, 404);
  });

test("dev bootstrap is loopback-only and keeps the token out of the saved config",
  { timeout: 15000 }, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "relay-bootstrap-"));
    const config = join(root, "bootstrap.json");
    const token = join(root, "connection-token");
    await writeFile(config, JSON.stringify({
      schemaVersion: 1,
      hosts: [{ label: "Demo Primary", endpoint: "ws://127.0.0.1:42000" }],
    }) + "\n");
    const fixtureToken = "<fixture-token>";
    await writeFile(token, fixtureToken + "\n", { mode: 0o600 });
    const probe = createServer();
    probe.listen(0, "127.0.0.1");
    await once(probe, "listening");
    const port = probe.address().port;
    probe.close();
    await once(probe, "close");
    const child = spawn(process.execPath, ["src/serve.mjs"], {
      cwd: process.cwd(),
      env: {
        ...process.env,
        HOST: "127.0.0.1",
        PORT: String(port),
        RELAY_DEMO_BOOTSTRAP_FILE: config,
        RELAY_DEMO_TOKEN_FILE: token,
      },
      stdio: "ignore",
    });
    t.after(async () => {
      child.kill();
      if (child.exitCode === null) await once(child, "exit");
      await rm(root, { recursive: true, force: true });
    });
    let response;
    for (let attempt = 0; attempt < 60; attempt++) {
      try {
        response = await fetch(`http://127.0.0.1:${port}/dev-hosts.json`);
        break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    }
    assert.equal(response?.status, 200);
    assert.deepEqual(await response.json(), {
      schemaVersion: 1,
      hosts: [{ label: "Demo Primary", endpoint: "ws://127.0.0.1:42000", token: fixtureToken }],
    });
    assert.doesNotMatch(await readFile(config, "utf8"), new RegExp(fixtureToken));
  });

test("HOST=0.0.0.0 exposes the HTTP listener beyond 127.0.0.1", {
  timeout: 15000, skip: process.platform !== "linux",
}, async () => {
  const socket = createServer();
  socket.listen(0, "127.0.0.1");
  await once(socket, "listening");
  const port = socket.address().port;
  await new Promise((resolve, reject) =>
    socket.close((error) => error ? reject(error) : resolve()));

  const child = spawn(process.execPath, ["src/serve.mjs"], {
    cwd: process.cwd(),
    env: { ...process.env, HOST: "0.0.0.0", PORT: String(port) },
    stdio: "ignore",
  });
  try {
    let responded = false;
    for (let attempt = 0; attempt < 60; attempt++) {
      if (child.exitCode !== null) throw Error("Relay exited before opening its listener");
      try {
        const response = await fetch(`http://127.0.0.2:${port}/`, {
          signal: AbortSignal.timeout(500),
        });
        assert.equal(response.status, 200);
        responded = true;
        break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    }
    assert.equal(responded, true, "Wildcard listener must also accept loopback alias 127.0.0.2");
  } finally {
    child.kill();
    if (child.exitCode === null) await once(child, "exit");
  }
});

test("HOST rejects hostnames instead of silently binding unexpected network interfaces", () => {
  const result = spawnSync(process.execPath, ["src/serve.mjs"], {
    cwd: process.cwd(),
    env: { ...process.env, HOST: "example.com" },
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /HOST must be a literal IPv4 or IPv6 address/);
});
