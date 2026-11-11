import { spawnSync } from "node:child_process";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const outDir = await mkdtemp(join(tmpdir(), "relay-test-build-"));
const env = { ...process.env, RELAY_BUILD_DIR: outDir };
const run = (args) => {
  const result = spawnSync(process.execPath, args, { env, stdio: "inherit" });
  if (result.error) throw result.error;
  return result.status ?? 1;
};

try {
  const built = run(["scripts/build.mjs"]);
  if (built) process.exitCode = built;
  else {
    const files = (await readdir("test"))
      .filter((file) => file.endsWith(".test.mjs") && file !== "relay-ui.test.mjs")
      .sort().map((file) => join("test", file));
    if (!files.length) throw Error("No tests found");
    process.exitCode = run(["--test", ...files]);
  }
} finally {
  await rm(outDir, { recursive: true, force: true });
}
