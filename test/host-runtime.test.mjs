import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, lstat, readFile, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  ROOT, DEMO_ROOT, DEMO_BOOTSTRAP, DEMO_HOSTS, DEMO_TEMPLATE, atomicWrite,
  rotateToken, restoreToken,
} from "../scripts/host-runtime.mjs";

test("demo host topology uses fixed, distinct ports without shadowing production", () => {
  assert.deepEqual(DEMO_HOSTS.map(({ port }) => port), [42000, 42001]);
  assert.equal(DEMO_HOSTS.reduce((total, host) => total + host.sessions, 0), 24);
  assert.equal(new Set(DEMO_HOSTS.map((host) => host.unit)).size, 2);
  assert.ok(DEMO_ROOT.startsWith(ROOT + "/"));
  assert.equal(DEMO_BOOTSTRAP, join(DEMO_ROOT, "bootstrap.json"));
  assert.equal(DEMO_TEMPLATE, join(ROOT, "deploy", "relay-demo@.service"));
});

test("tokens rotate atomically without leaving readable files or overwriting symlinks",
  async (t) => {
    const root = await mkdtemp(join(tmpdir(), "relay-token-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    const file = join(root, "private", "connection-token");
    const first = await rotateToken(file);
    assert.match(first.token, /^[a-f0-9]{64}$/);
    assert.equal(first.previous, null);
    assert.equal((await lstat(file)).mode & 0o777, 0o600);
    assert.equal((await lstat(join(root, "private"))).mode & 0o777, 0o700);
    const second = await rotateToken(file);
    assert.notEqual(first.token, second.token);
    assert.equal(second.previous.toString("utf8"), first.token + "\n");
    assert.equal((await readFile(file, "utf8")).trim(), second.token);
    await restoreToken(file, second.previous);
    assert.equal((await readFile(file, "utf8")).trim(), first.token);
    const victim = join(root, "victim");
    await atomicWrite(victim, "leave untouched");
    const link = join(root, "symlink-token");
    await symlink(victim, link);
    await assert.rejects(rotateToken(link), /unsafe/);
    assert.equal(await readFile(victim, "utf8"), "leave untouched");
    await restoreToken(file, null);
    await assert.rejects(lstat(file), { code: "ENOENT" });
  });
