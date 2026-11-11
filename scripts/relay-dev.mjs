import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const relay = join(dirname(fileURLToPath(import.meta.url)), "..");

process.env.HOST = process.env.HOST || "0.0.0.0";
process.env.PORT = process.env.PORT || "8080";
process.env.RELAY_PREVIEW = process.env.RELAY_PREVIEW || "1";

const build = spawn(process.execPath, ["scripts/build.mjs"], {
  cwd: relay,
  stdio: "inherit",
});
const code = await new Promise((resolve, reject) => {
  build.on("error", reject);
  build.on("exit", resolve);
});
if (code !== 0) process.exit(code ?? 1);

await import(join(relay, "src/serve.mjs"));
