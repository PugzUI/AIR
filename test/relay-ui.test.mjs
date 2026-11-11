import test from "node:test";
import assert from "node:assert/strict";
import { topologyFocusId } from "../src/features/relay/helpers.ts";

test("filtered topology retains a keyboard-reachable node after selection disappears", () => {
  const nodes = [{id:"secondary"},{id:"secondary-session"}];
  assert.equal(topologyFocusId(nodes,"primary","primary-session"),"secondary");
});

test("topology focus prefers an existing focused node, then selection, then the first node", () => {
  const nodes = [{id:"first"},{id:"selected"},{id:"focused"}];
  assert.equal(topologyFocusId(nodes,"focused","selected"),"focused");
  assert.equal(topologyFocusId(nodes,"missing","selected"),"selected");
  assert.equal(topologyFocusId(nodes),"first");
  assert.equal(topologyFocusId([],"missing","missing"),undefined);
});
