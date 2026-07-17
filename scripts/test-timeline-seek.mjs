import assert from "node:assert/strict";

import { iinaTimelineSeekPlan } from "../src/timeline-seek.js";

assert.deepEqual(iinaTimelineSeekPlan(30, 120, false, 0), {
  type: "seek-percent",
  percent: 25,
  exact: true,
});
assert.equal(iinaTimelineSeekPlan(30, 120, true, 0).exact, false);
assert.equal(iinaTimelineSeekPlan(30, 120, true, 1).exact, true);
assert.equal(iinaTimelineSeekPlan(30, 120, true, 2).exact, true);
assert.equal(iinaTimelineSeekPlan(30, 120, true, "0").exact, false);
assert.equal(iinaTimelineSeekPlan(10, 0, false, 0).percent, 0);

console.log("Timeline seek contracts pass");
