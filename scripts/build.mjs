import { build } from "esbuild";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

const outDir = process.env.RELAY_BUILD_DIR || "dist";
await mkdir(outDir, { recursive: true });
const result = await build({
  entryPoints: ["ui/app.mjs"],
  outfile: join(outDir, "app.js"),
  bundle: true,
  minify: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  legalComments: "eof",
  write: false,
});
const artifacts = [
  ["app.js", result.outputFiles[0].contents],
  ["index.html", await readFile("ui/index.html")],
  ["style.css", await readFile("ui/style.css")],
];
// Node serves these files live. Publish complete assets with same-directory
// atomic renames so a reload never receives half-written JavaScript or CSS.
const pending = [];
try {
  for (const [name, bytes] of artifacts) {
    const destination = join(outDir, name);
    try {
      if (Buffer.compare(await readFile(destination), Buffer.from(bytes)) === 0) continue;
    } catch {}
    const file = join(outDir, `.${name}.${process.pid}.${randomUUID()}.tmp`);
    await writeFile(file, bytes, { mode: 0o644 });
    pending.push([file, destination]);
  }
  for (const [file, destination] of pending) await rename(file, destination);
} finally {
  await Promise.all(pending.map(([file]) => rm(file, { force: true })));
}
console.log("Relay fleet console built");
