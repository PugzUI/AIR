import test from "node:test";
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { isolatedFleet, launchBrowser } from "./fixtures/isolated-hosts.mjs";

test("browser runs ACP turns, approvals, cancel, two hosts and restart boundaries",
  { timeout: 150000 }, async (t) => {
    const { hosts, webOrigin } = await isolatedFleet(t, { hostCount: 2, withWeb: true });
    const browser = await launchBrowser();
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const pageErrors = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));
    await page.addInitScript(() => {
      window.__cspViolations = [];
      document.addEventListener("securitypolicyviolation", (event) => {
        window.__cspViolations.push(event.effectiveDirective);
      });
    });

    async function connect(label, host) {
      const ensureHostPanel = async () => {
        if (await page.locator(".host-panel-toggle").getAttribute("aria-expanded") !== "true") {
          await page.locator(".host-panel-toggle").click();
        }
      };
      if (!(await page.locator(".connect-form").count())) {
        await page.locator(".bar-add-host").click();
      }
      await page.locator('input[name="label"]').fill(label);
      await page.locator('input[name="endpoint"]').fill(host.origin);
      await page.locator('input[name="token"]').fill(host.token);
      await page.locator('.connect-form button[type="submit"]').click();
      await page.waitForFunction(() => !document.querySelector("#connect-panel") && !document.querySelector(".side-drawer")?.open);
      await ensureHostPanel();
      await page.getByRole("button", { name: new RegExp(label, "i") })
        .filter({ has: page.locator(".host-text") }).waitFor({ state: "visible" });
      await page.waitForFunction((name) => {
        const item = [...document.querySelectorAll(".host-row")]
          .find((row) => row.querySelector("strong")?.textContent === name);
        return item?.querySelector("small")?.textContent === "connected";
      }, label, { timeout: 20000 });
    }

    async function createSession(label, host) {
      if (await page.locator(".side-drawer").evaluate((drawer) => drawer.open)) {
        await page.locator(".drawer-close").click();
      }
      await page.locator(".host-row").filter({ hasText: label }).click();
      await page.locator(".drawer-close").click();
      await page.locator(".sessions-panel button.primary").click();
      await page.locator('.create-form input[name="directory"]')
        .fill(pathToFileURL(host.workspace).href);
      await page.locator('.workspace-trust input[name="trustWorkspace"]').check();
      await page.locator(".create-form button.primary").click();
      await page.locator(".composer textarea").waitFor({ state: "visible" });
      await page.waitForFunction((hostId) => {
        const fleet = document.querySelector("relay-fleet")?.fleet;
        return fleet?.selected?.hostId === hostId &&
          fleet.slots.chat?.ready && !!fleet.chat;
      }, label.toLowerCase(), { timeout: 20000 });
    }

    async function prompt(message) {
      await page.locator(".composer textarea").fill(message);
      await page.locator('.composer button[type="submit"]').click();
    }

    async function turnState(message, state) {
      await page.waitForFunction(({ prompt, outcome }) =>
        [...document.querySelectorAll(".turn")].some((turn) =>
          turn.querySelector(".turn-prompt")?.textContent === prompt &&
          turn.querySelector(".turn-outcome")?.textContent?.toLowerCase().includes(outcome)),
      { prompt: message, outcome: state }, { timeout: 20000 });
    }

    try {
      await page.goto(webOrigin);
      assert.equal(await page.locator(".sidebar .rail-data").count(), 0,
        "Sidebar must not duplicate the four authoritative main metrics");
      assert.equal(await page.locator(".metrics > div").count(), 0,
        "The shell must not repeat metrics in four oversized cards");
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), false);
      assert.equal(await page.locator(".session-list").count(), 0,
        "Session listing starts collapsed to keep the topology primary");
      assert.equal(await page.locator(".sidebar").getByText("FLEET SUMMARY").count(), 0);
      assert.equal(await page.locator(".sidebar").getByText("No connected hosts. Add an AHP endpoint.").count(), 0);
      assert.equal(await page.locator(".map-head .topology-keys").count(), 1,
        "Keys sits at the right of the topology legend divider");
      assert.equal(await page.locator(".section-head .topology-keys").count(), 0);
      assert.equal(await page.locator(".inspector").count(), 0,
        "An empty inspector must not occupy the shell");
      assert.equal(await page.locator(".empty-map span").count(), 0,
        "The empty topology already explains itself without a second instruction");
      const mono = await page.locator(".rail-title").evaluate((node) =>
        getComputedStyle(node).fontFamily);
      assert.match(mono, /^"Geist Mono"/);
      await page.locator(".topology-keys").click();
      assert.equal(await page.locator(".shortcuts-dialog").evaluate((dialog) => dialog.open), true);
      await page.keyboard.press("Escape");
      assert.equal(await page.locator(".shortcuts-dialog").evaluate((dialog) => dialog.open), false);
      // Host creation expands inline on the topology separator bar without opening the right drawer.
      await page.locator(".bar-add-host").click();
      await page.locator(".map-head .connect-form").waitFor({ state: "visible" });
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), false);
      assert.equal(await page.locator('input[name="label"]').evaluate((input) =>
        document.activeElement === input), true);
      await connect("Alpha", hosts[0]);
      await connect("Beta", hosts[1]);
      await page.waitForFunction(() =>
        document.querySelectorAll(".host-row small").length === 2 &&
        [...document.querySelectorAll(".host-row small")]
          .every((node) => node.textContent === "connected"));
      await page.locator(".bar-add-host").click();
      await page.waitForFunction(() => document.activeElement ===
        document.querySelector('#connect-panel input[name="label"]'));
      assert.equal(await page.locator(".host-row").count(), 2,
        "Opening the connection form must not disrupt existing hosts");
      await page.locator(".connect-close").click();

      await createSession("Alpha", hosts[0]);
      await createSession("Beta", hosts[1]);
      await page.locator(".drawer-close").click();
      await page.waitForFunction(() => !document.querySelector(".side-drawer")?.open);
      const previewNode = page.locator("svg.map .node.session").last();
      await previewNode.click();
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), false,
        "Clicking a topology node must stay in preview mode");
      await page.locator(".node-preview").waitFor({ state: "visible" });
      assert.equal(await page.locator(".node-preview-action", { hasText: "Control" }).count(), 1);
      await page.locator(".node-preview .preview-close").click();
      await page.locator(".node-preview").waitFor({ state: "hidden" });
      await previewNode.click();
      await previewNode.press("Enter");
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), true,
        "Enter on the previewed node must open its controls");
      await page.locator(".drawer-close").click();
      await page.waitForFunction(() => !document.querySelector(".side-drawer")?.open);
      await previewNode.click();
      await page.locator(".node-preview-action", { hasText: "Control" }).click();
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), true,
        "Clicking Control -> on the preview card must open its controls");
      assert.equal(await page.locator(".drawer-tools button").filter({ hasText: "Refresh" }).count(), 1,
        "Session refresh is accessible only in the drawer");
      assert.equal(await page.locator("svg.map .node-meta").count(), 0,
        "Topology nodes expose metadata on demand instead of rendering a second line");
      assert.equal(await page.locator(".session-list").count(), 0,
        "Drawer activity must not expand the session catalog");
      await page.waitForFunction(() =>
        document.querySelectorAll("svg.map .node.host").length === 2 &&
        document.querySelectorAll("svg.map .node.session").length === 2,
      null, { timeout: 20000 });
      assert.equal(await page.locator(".map-lightfield").count(), 1,
        "Live topology must retain the original shader-lightfield layer");
      const lightfield = await page.locator(".map-lightfield").evaluate((canvas) => {
        const gl = canvas.getContext("webgl");
        if (!gl) return { supported: false };
        const pixels = new Uint8Array(canvas.width * canvas.height * 4);
        gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        let maxLight = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          maxLight = Math.max(maxLight, pixels[i], pixels[i + 1], pixels[i + 2]);
        }
        return { supported: true, error: gl.getError(), maxLight };
      });
      if (lightfield.supported) {
        assert.equal(lightfield.error, 0, "Neon lightfield must not produce WebGL errors");
        assert.ok(lightfield.maxLight > 24,
          "Neon paths must actually render visible light, not a black framebuffer");
      }
      await page.getByRole("tab", { name: "Files", exact: true }).click();
      assert.equal(await page.locator(".composer").count(), 0,
        "Only the requested drawer detail view should be present");
      await page.getByRole("tab", { name: "Info", exact: true }).click();
      assert.ok(await page.locator(".inspector .details").count() > 0);
      await page.getByRole("tab", { name: "Conversation", exact: true }).click();
      assert.equal(await page.locator(".composer").count(), 1);

      await prompt("Normal Beta turn.");
      await turnState("Normal Beta turn.", "complete");
      assert.match(await page.locator(".response-text").last().innerText(), /Test turn complete/);

      await prompt("Check approval denial.");
      await page.locator(".attention-item button").filter({ hasText: "Deny this operation" })
        .waitFor({ state: "visible", timeout: 15000 });
      await page.locator(".attention-item button").filter({ hasText: "Deny this operation" }).click();
      await turnState("Check approval denial.", "complete");
      assert.match(await page.locator(".response-text").last().innerText(), /Permission deny/);

      await prompt("Check approval once.");
      const allow = page.locator(".attention-item button").filter({ hasText: "Allow once" });
      await allow.waitFor({ state: "visible", timeout: 15000 });
      await allow.click();
      await turnState("Check approval once.", "complete");
      assert.match(await page.locator(".response-text").last().innerText(), /Permission approve-once/);

      await prompt("Check cancel now.");
      await page.locator(".composer-actions button.quiet")
        .waitFor({ state: "visible", timeout: 15000 });
      await page.waitForFunction(() => !document.querySelector(".composer-actions button.quiet")?.disabled,
        null, { timeout: 15000 });
      await page.locator(".composer-actions button.quiet").click();
      await turnState("Check cancel now.", "cancelled");
      await page.waitForFunction(() =>
        document.querySelector(".inspector-title .state-text")?.textContent?.trim() === "idle",
      null, { timeout: 15000 });

      await page.locator(".drawer-close").click();
      await page.locator(".host-row").filter({ hasText: "Beta" }).click();
      await page.locator(".host-detail .rail-actions button").filter({ hasText: "Reconnect" }).click();
      await page.waitForFunction(() =>
        document.querySelector(".host-row.selected small")?.textContent === "connected",
      null, { timeout: 30000 });
      await page.locator(".drawer-close").click();
      await page.locator(".node-preview-action").click();
      await page.waitForFunction(() => [...document.querySelectorAll(".turn-outcome")]
        .some((node) => /cancelled/i.test(node.textContent)), null, { timeout: 30000 });
      await prompt("After Beta reconnect.");
      await turnState("After Beta reconnect.", "complete");

      await page.locator(".drawer-close").click();
      await page.locator(".catalog-toggle").click();
      await page.locator(".session-row").filter({ hasText: "Alpha" }).click();
      await page.locator(".composer textarea").waitFor({ state: "visible" });
      await page.waitForFunction(() =>
        document.querySelector(".transcript")?.textContent?.includes("No turns in this chat."));
      await prompt("Before Alpha restart.");
      await turnState("Before Alpha restart.", "complete");

      await hosts[0].stop();
      await page.waitForFunction(() => {
        const alpha = [...document.querySelectorAll(".host-row")]
          .find((row) => row.querySelector("strong")?.textContent === "Alpha");
        return alpha?.querySelector("small")?.textContent !== "connected" &&
          document.querySelector(".composer textarea")?.disabled;
      }, null, { timeout: 20000 });
      await hosts[0].start();
      await page.waitForFunction(() => {
        const alpha = [...document.querySelectorAll(".host-row")]
          .find((row) => row.querySelector("strong")?.textContent === "Alpha");
        return alpha?.querySelector("small")?.textContent === "connected" &&
          document.querySelector(".composer textarea")?.disabled &&
          document.querySelector(".inspector-title .state-text")?.textContent === "UNAVAILABLE" &&
          document.querySelector(".transcript")?.textContent?.includes("No agent for session");
      }, null, { timeout: 30000 }).catch(async (error) => {
        const state = await page.evaluate(() => {
          const fleet = document.querySelector("relay-fleet")?.fleet;
          return {
            hosts: [...document.querySelectorAll(".host-row")].map((node) => node.innerText),
            selected: fleet?.selected,
            sessionReady: !!fleet?.slots.session?.ready,
            chatReady: !!fleet?.slots.chat?.ready,
            sessionError: fleet?.slots.session?.error,
            chatError: fleet?.slots.chat?.error,
            transcript: document.querySelector(".transcript")?.innerText?.slice(0, 300),
            composerDisabled: document.querySelector(".composer textarea")?.disabled,
          };
        });
        throw Error(`AHP unavailable-session recovery: ${error.message}; state=${JSON.stringify(state)}`);
      });
      // This fixture deliberately advertises loadSession: false. ahpd reconnects,
      // but cannot restore its old ACP process. Relay must retain that boundary.
      await createSession("Alpha", hosts[0]);
      await prompt("Fresh session after Alpha restart.");
      await turnState("Fresh session after Alpha restart.", "complete");

      assert.equal(await page.locator(".host-row").count(), 2);
      await page.locator(".drawer-close").click();
      await page.setViewportSize({ width: 390, height: 844 });
      const mobile = await page.evaluate(() => ({
        viewport: window.innerWidth,
        document: document.documentElement.scrollWidth,
      }));
      assert.ok(mobile.document <= mobile.viewport + 1,
        `Mobile layout overflows the viewport: ${JSON.stringify(mobile)}`);
      const addHostMobile = await page.locator(".bar-add-host")
        .evaluate((button) => ({ minHeight: button.getBoundingClientRect().height,
          right: button.getBoundingClientRect().right,
          viewport: window.innerWidth }));
      assert.ok(addHostMobile.minHeight >= 44 &&
        addHostMobile.right <= addHostMobile.viewport + 1,
      `The mobile Add host action is clipped or too small: ${JSON.stringify(addHostMobile)}`);
      assert.equal(await page.locator("svg.map .node.host").count(), 2);

      await page.emulateMedia({ reducedMotion: "reduce" });
      assert.equal(await page.locator(".map-lightfield").evaluate((canvas) =>
        getComputedStyle(canvas).display), "none",
      "Reduced-motion users keep the accessible SVG without the shader");
      await page.emulateMedia({ reducedMotion: "no-preference" });

      await page.setViewportSize({ width: 320, height: 900 });
      await page.locator(".rail-close").click();
      await page.locator(".node-preview-action").click();
      await page.evaluate(() => { document.documentElement.style.fontSize = "32px"; });
      const zoom = await page.evaluate(() => ({
        overflow: document.documentElement.scrollWidth - innerWidth,
        closeRight: document.querySelector(".drawer-close").getBoundingClientRect().right,
        viewport: innerWidth,
      }));
      assert.ok(zoom.overflow <= 1 && zoom.closeRight <= zoom.viewport + 1,
        `The open drawer must reflow at 320px with 200% text: ${JSON.stringify(zoom)}`);
      await page.evaluate(() => { document.documentElement.style.fontSize = ""; });
      await page.locator(".drawer-close").click();

      await page.setViewportSize({ width: 1440, height: 900 });
      await page.locator(".node-preview-action").click();
      await page.getByRole("tab", { name: "Info", exact: true }).click();
      page.once("dialog", (dialog) => dialog.accept());
      await page.locator(".inspector-footer button.danger").click();
      await page.waitForFunction(() => !document.querySelector("relay-fleet")?.fleet.selected, null,
        { timeout: 15000 });
      assert.equal(await page.locator(".side-drawer").evaluate((drawer) => drawer.open), false);
      assert.deepEqual(pageErrors, []);
      assert.deepEqual(await page.evaluate(() => window.__cspViolations), [],
        "The production console must not rely on blocked inline code, styles, or unsafe media");
      // Credentials are not exported into page content or browser storage.
      const clientState = await page.evaluate(() => ({
        text: document.documentElement.outerHTML,
        local: JSON.stringify(localStorage),
      }));
      for (const host of hosts) {
        assert.ok(!clientState.text.includes(host.token));
        assert.ok(!clientState.local.includes(host.token));
      }
    } finally {
      await browser.close();
    }
  });

test("missing or invalid token can be repaired with the same host name", { timeout: 45000 }, async (t) => {
  const { hosts, webOrigin } = await isolatedFleet(t, { withWeb: true });
  const browser = await launchBrowser();
  try {
    const page = await browser.newPage({ viewport: { width: 960, height: 720 } });
    await page.goto(webOrigin);
    await page.locator(".bar-add-host").click();
    await page.locator('input[name="label"]').fill("example-host");
    await page.locator('input[name="endpoint"]').fill(hosts[0].origin);
    await page.locator('.connect-form button[type="submit"]').click();
    assert.equal(await page.locator('input[name="token"]').evaluate((input) =>
      input.validity.valueMissing), true);
    assert.equal(await page.locator(".host-row").count(), 0);

    await page.locator('input[name="token"]').fill("invalid-token");
    await page.locator('.connect-form button[type="submit"]').click();
    const connectClose = page.locator(".connect-close");
    if (await connectClose.isVisible()) await connectClose.click();
    if (await page.locator(".host-panel-toggle").getAttribute("aria-expanded") !== "true") {
      await page.locator(".host-panel-toggle").click();
    }
    await page.locator(".host-row").filter({ hasText: "example-host" }).click();
    await page.waitForFunction(() =>
      document.querySelector(".host-error")?.textContent?.includes("Check the host URL and access token"),
    null, { timeout: 15000 });
    // Retry from the Connect form without deleting the failed example-host entry.
    // The previous behavior raised "Host name is already in use".
    await page.locator(".host-detail button").filter({ hasText: "Update credentials" }).click();
    await page.locator('input[name="label"]').fill("example-host");
    await page.locator('input[name="endpoint"]').fill(hosts[0].origin);
    await page.locator('input[name="token"]').fill(hosts[0].token);
    await page.locator('.connect-form button[type="submit"]').click();
    await page.waitForFunction(() =>
      document.querySelector(".host-row small")?.textContent === "connected", null, { timeout: 15000 });
    assert.equal(await page.locator(".host-error").count(), 0);
    assert.equal(await page.locator(".host-row").count(), 1, "Retry must not duplicate the host");
    // Re-submitting that name once connected must not interrupt its sessions.
    await page.locator(".bar-add-host").click();
    await page.locator('input[name="label"]').fill("example-host");
    await page.locator('input[name="endpoint"]').fill(hosts[0].origin);
    await page.locator('input[name="token"]').fill(hosts[0].token);
    await page.locator('.connect-form button[type="submit"]').click();
    await page.waitForFunction(() =>
      document.querySelector(".alert")?.textContent?.includes("already connected"), null,
      { timeout: 10000 });
    assert.equal(((await page.locator(".host-row small").textContent()) || "").toLowerCase(), "connected");
    assert.equal(await page.locator(".host-row").count(), 1);

    // The token's instructions remain available to assistive technology, but
    // do not shift the input or button or add visual clutter at any viewport.
    for (const width of [1440, 1280, 1161, 1160, 1101, 1100, 900, 760, 540, 390, 360]) {
      await page.setViewportSize({ width, height: 900 });
      const positions = await page.evaluate(() => {
        const rect = (selector) => document.querySelector(selector).getBoundingClientRect();
        const form = rect(".connect-form");
        const token = rect('.connect-form input[name="token"]');
        const button = rect('.connect-form button[type="submit"]');
        const help = document.querySelector("#host-token-help");
        return {
          rightInset: window.innerWidth - button.right,
          withinForm: button.right <= form.right + 1,
          separated: button.left >= token.right - 1 || button.top >= token.bottom - 1,
          helpVisuallyHidden: getComputedStyle(help).position === "absolute" &&
            help.getBoundingClientRect().width <= 1,
          hintAssociation: document.querySelector('.connect-form input[name="token"]')
            .getAttribute("aria-describedby") === "host-token-help",
          overflow: document.documentElement.scrollWidth > window.innerWidth + 1,
        };
      });
      assert.ok(positions.rightInset >= 8 && positions.withinForm,
        `Connect button clips the form edge at ${width}px: ${JSON.stringify(positions)}`);
      assert.ok(positions.separated && !positions.overflow,
        `Connect form overlaps or overflows at ${width}px: ${JSON.stringify(positions)}`);
      assert.ok(positions.helpVisuallyHidden && positions.hintAssociation,
        `Token hint is visually distracting or loses accessible labeling at ${width}px`);
    }
  } finally {
    await browser.close();
  }
});

test("top-down topology stays bounded, pannable, and keeps neon lighting aligned", { timeout: 45000 }, async (t) => {
  const { webOrigin } = await isolatedFleet(t, { withWeb: true });
  const browser = await launchBrowser();
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 750 } });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(webOrigin);
    // Deliberately injected presentation fixture: it cannot create real AHP
    // sessions or agent traffic and never ships with the browser application.
    await page.evaluate(() => {
      const element = document.querySelector("relay-fleet");
      const hosts = Array.from({ length: 4 }, (_, hostIndex) => ({
        id: "fixture-" + hostIndex,
        label: "Fixture " + hostIndex,
        state: { status: "connected" },
        sessionSummaries: [],
        agents: Array.from({ length: 30 }, (_, agentIndex) => ({
          provider: "agent-" + agentIndex,
          displayName: "Agent " + agentIndex,
        })),
      }));
      element.fleet.hosts = () => hosts;
      element.fleet.sessions = () => [];
      element.requestUpdate();
    });
    await page.waitForFunction(() => {
      const graph = document.querySelector("relay-fleet")?.visibleGraph;
      return graph?.height > 700 && graph.height < 2200 && graph.width > 1280;
    });
    const viewport = page.locator(".map-viewport");
    const canvas = page.locator(".map-lightfield");
    const layout = await page.evaluate(() => {
      const graph = document.querySelector("relay-fleet")?.visibleGraph;
      const viewport = document.querySelector(".map-viewport");
      const stage = document.querySelector(".map-stage");
      return {
        graph,
        viewportOverflow: getComputedStyle(viewport).overflow,
        documentOverflow: document.documentElement.scrollHeight - window.innerHeight,
        stageTransform: stage?.style.transform,
      };
    });
    assert.equal(layout.viewportOverflow, "hidden");
    assert.ok(layout.documentOverflow <= 1,
      "The topology must not expose a document scrollbar");

    const panState = (stage) => `${stage.getAttribute("data-pan-x")},${stage.getAttribute("data-pan-y")}`;
    const beforePan = await page.locator(".map-stage").evaluate(panState);
    const viewportBox = await viewport.boundingBox();
    assert.ok(viewportBox);
    await page.mouse.move(viewportBox.x + 8, viewportBox.y + viewportBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(viewportBox.x + 92, viewportBox.y + viewportBox.height / 2 + 74);
    await page.mouse.up();
    const afterPan = await page.locator(".map-stage").evaluate(panState);
    assert.notEqual(afterPan, beforePan, "Dragging empty map space must pan the stage");

    const provider = page.locator("svg.map .node.provider").first();
    const providerBox = await provider.boundingBox();
    assert.ok(providerBox);
    await viewport.focus();
    await page.keyboard.down(" ");
    await page.mouse.move(providerBox.x + providerBox.width / 2, providerBox.y + providerBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(providerBox.x + providerBox.width / 2 + 56,
      providerBox.y + providerBox.height / 2 + 36);
    await page.mouse.up();
    await page.keyboard.up(" ");
    const afterNodePan = await page.locator(".map-stage").evaluate(panState);
    assert.notEqual(afterNodePan, afterPan, "Holding Space must pan over a node");

    const result = await canvas.evaluate((element) => {
      const gl = element.getContext("webgl");
      if (!gl) return { supported: false };
      const pixels = new Uint8Array(element.width * element.height * 4);
      gl.readPixels(0, 0, element.width, element.height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      let maxLight = 0;
      for (let i = 0; i < pixels.length; i += 4) {
        maxLight = Math.max(maxLight, pixels[i], pixels[i + 1], pixels[i + 2]);
      }
      return { supported: true, width: element.width, height: element.height, maxLight,
        error: gl.getError(), transform: element.style.transform };
    });
    if (result.supported) {
      assert.equal(result.error, 0);
      assert.ok(result.maxLight > 24, `Top-down map lightfield is blank: ${JSON.stringify(result)}`);
      assert.equal(result.width, layout.graph.width);
      assert.equal(result.height, layout.graph.height);
      assert.equal(result.transform, "");
    }
    assert.deepEqual(errors, []);
  } finally {
    await browser.close();
  }
});

test("external session disposal clears every viewer without leaving a live ghost", { timeout: 60000 }, async (t) => {
  const { hosts, webOrigin } = await isolatedFleet(t, { withWeb: true });
  const browser = await launchBrowser();
  try {
    const pages = [await browser.newPage(), await browser.newPage()];
    for (const page of pages) {
      await page.goto(webOrigin);
      await page.locator(".bar-add-host").click();
      await page.locator('input[name="label"]').fill("Shared");
      await page.locator('input[name="endpoint"]').fill(hosts[0].origin);
      await page.locator('input[name="token"]').fill(hosts[0].token);
      await page.locator(".connect-form button[type=submit]").click();
      await page.waitForFunction(() =>
        document.querySelector(".host-row small")?.textContent === "connected");
    }
    const [owner, other] = pages;
    await owner.locator(".host-panel-toggle").click();
    await owner.locator(".host-row").click();
    await owner.locator(".drawer-close").click();
    await owner.locator(".sessions-panel button.primary").click();
    await owner.locator('.create-form input[name="directory"]')
      .fill(pathToFileURL(hosts[0].workspace).href);
    await owner.locator(".create-form button.primary").click();
    await owner.waitForFunction(() => !!document.querySelector("relay-fleet")?.fleet.chat);
    const selectedUri = await owner.evaluate(() =>
      document.querySelector("relay-fleet").fleet.selected.uri);
    await other.waitForFunction((uri) =>
      document.querySelector("relay-fleet")?.fleet.sessions()
        .some((row) => row.summary.resource === uri), selectedUri);
    await other.locator(".catalog-toggle").click();
    await other.locator(".session-row").click();
    await other.getByRole("tab", { name: "Info", exact: true }).click();
    other.once("dialog", (dialog) => dialog.accept());
    await other.locator(".inspector-footer button.danger").click();
    await owner.waitForFunction((uri) => {
      const relay = document.querySelector("relay-fleet");
      return relay.fleet.selected === null &&
        !relay.fleet.sessions().some((row) => row.summary.resource === uri) &&
        !relay.visibleGraph.nodes.some((node) => node.uri === uri) &&
        !document.querySelector(".side-drawer").open;
    }, selectedUri, { timeout: 20000 });
  } finally {
    await browser.close();
  }
});

test("host-defined structured session options are editable without an inline shell form",
  { timeout: 45000 }, async (t) => {
    const { hosts, webOrigin } = await isolatedFleet(t, { withWeb: true });
    const browser = await launchBrowser();
    try {
      const page = await browser.newPage({ viewport: { width: 390, height: 840 } });
      await page.goto(webOrigin);
      await page.locator(".bar-add-host").click();
      await page.locator('input[name="label"]').fill("Host");
      await page.locator('input[name="endpoint"]').fill(hosts[0].origin);
      await page.locator('input[name="token"]').fill(hosts[0].token);
      await page.locator(".connect-form button[type=submit]").click();
      await page.waitForFunction(() =>
        document.querySelector(".host-row small")?.textContent === "connected");
      await page.locator(".host-panel-toggle").click();
      await page.locator(".host-row").click();
      await page.locator(".drawer-close").click();
      await page.locator(".sessions-panel button.primary").click();
      await page.waitForFunction(() => !!document.querySelector("relay-fleet").sessionConfig);
      await page.evaluate(() => {
        const app = document.querySelector("relay-fleet");
        app.sessionConfig = {
          values: {},
          schema: {
            required: ["branching", "weights"],
            properties: {
              branching: { type: "object", title: "Branch settings" },
              weights: { type: "array", title: "Weights",
                items: { type: "number", title: "Weight" } },
            },
          },
        };
        app.requestUpdate();
      });
      await page.locator('[name="config:branching"]').fill('{"base":"main","count":2}');
      await page.locator('[name="config:weights"]').fill("[1,2,3]");
      const parsed = await page.evaluate(() => {
        const app = document.querySelector("relay-fleet");
        return app.readSessionConfig(new FormData(document.querySelector(".create-form")));
      });
      assert.deepEqual(parsed, { branching: { base: "main", count: 2 }, weights: [1, 2, 3] });
      await page.locator('[name="config:branching"]').fill("{bad");
      const error = await page.evaluate(() => {
        try {
          const app = document.querySelector("relay-fleet");
          app.readSessionConfig(new FormData(document.querySelector(".create-form")));
          return "";
        } catch (caught) { return caught.message; }
      });
      assert.match(error, /Invalid JSON for host option/);
      assert.equal(await page.locator(".center .create-form").count(), 0);
    } finally {
      await browser.close();
    }
  });
