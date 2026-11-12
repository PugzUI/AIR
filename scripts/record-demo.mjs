import { chromium } from "playwright";
import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { spawnSync, execSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tempDir = resolve(rootDir, "temp-rec");
const docsDir = resolve(rootDir, "docs");

async function launch() {
  const options = {
    headless: true,
    args: [
      "--disable-dev-shm-usage",
      "--no-sandbox",
      "--enable-webgl",
      "--ignore-gpu-blocklist",
    ]
  };
  if (!existsSync(chromium.executablePath())) {
    const candidates = ["chromium", "google-chrome", "google-chrome-stable"];
    for (const candidate of candidates) {
      if (existsSync(candidate)) { options.executablePath = candidate; break; }
      const found = spawnSync("which", [candidate], { encoding: "utf8" });
      if (found.status === 0) { options.executablePath = found.stdout.trim(); break; }
    }
  }
  return chromium.launch(options);
}

rmSync(tempDir, { recursive: true, force: true });
mkdirSync(tempDir, { recursive: true });
mkdirSync(docsDir, { recursive: true });

console.log("Launching headless browser for recording...");
const browser = await launch();
const context = await browser.newContext({
  viewport: { width: 1280, height: 720 },
  deviceScaleFactor: 2,
  recordVideo: {
    dir: tempDir,
    size: { width: 1280, height: 720 },
  },
});

const page = await context.newPage();
await page.goto("http://127.0.0.1:4200/", { waitUntil: "domcontentloaded" });
await page.waitForSelector(".node", { timeout: 10000 });
await page.waitForTimeout(800);

// Let initial multi-agent packets flow across the network (1.4s)
await page.waitForTimeout(1400);

// Find a session node and interact with its tool rail
const targetNode = page.locator(".node.session").nth(1);
const box = await targetNode.boundingBox();
if (box) {
  const tool = targetNode.locator(".node-tool").first();
  const toolBox = await tool.boundingBox();
  const targetX = toolBox ? toolBox.x + toolBox.width / 2 : box.x + 8;
  const targetY = toolBox ? toolBox.y + toolBox.height / 2 : box.y + 12;

  // Move cursor naturally
  await page.mouse.move(targetX, targetY, { steps: 18 });
  await page.waitForTimeout(300);

  // Click the facet icon to switch to detail view (lime highlight, detail baselines)
  await tool.click();
  await page.waitForTimeout(1800);

  // Click to unpin and return to standard title view
  await tool.click();
  await page.waitForTimeout(300);

  // Move mouse away smoothly
  await page.mouse.move(40, 40, { steps: 14 });
}

// Complete the animation cycle
await page.waitForTimeout(1000);

await context.close();
await browser.close();

const recorded = readdirSync(tempDir).find((f) => f.endsWith(".webm"));
if (!recorded) {
  console.error("No webm recording found in", tempDir);
  process.exit(1);
}

const inputPath = resolve(tempDir, recorded);
const mp4Path = resolve(docsDir, "relay-demo.mp4");
const webpPath = resolve(docsDir, "relay-demo.webp");
const posterPath = resolve(docsDir, "demo-topology.png");

console.log("Encoding web-optimized MP4 (H.264)...");
execSync(
  `ffmpeg -ss 00:00:04 -to 00:00:20 -i "${inputPath}" -c:v libx264 -preset slow -crf 19 -pix_fmt yuv420p -movflags +faststart "${mp4Path}" -y`,
  { stdio: "inherit" }
);

console.log("Extracting high-resolution poster PNG...");
execSync(
  `ffmpeg -ss 00:00:15 -i "${inputPath}" -vframes 1 "${posterPath}" -y`,
  { stdio: "inherit" }
);

console.log("Encoding animated WebP (24-bit RGB)...");
execSync(
  `ffmpeg -ss 00:00:04 -to 00:00:20 -i "${inputPath}" -vcodec libwebp -filter:v "fps=20" -lossless 0 -compression_level 4 -q:v 75 -loop 0 "${webpPath}" -y`,
  { stdio: "inherit" }
);

// Clean up raw recording directory
rmSync(tempDir, { recursive: true, force: true });

console.log("Successfully generated moving demo assets in docs/:");
console.log("- " + mp4Path);
console.log("- " + webpPath);
console.log("- " + posterPath);
