// node --experimental-transform-types --test tests/file-selection.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { DeviceFiles } from "../src/device-files.ts";

function fixture(count = 6) {
  const controller = Object.create(DeviceFiles.prototype);
  const session = { id: "files", deviceId: "A", path: "/", entries: Array.from({ length: count }, (_, index) => ({ path: `/file-${index}`, name: `file-${index}`, kind: "file", is_dir: false })),
    selected: "", selectedNavigation: "", selectedPaths: new Set(), selectionAnchor: "", loading: false, busy: false, syncing: false, transfer: null, batch: null, dragging: false };
  controller.paintSelection = () => {};
  controller.sessions = new Map([[session.id, session]]);
  controller.hooks = { online: () => true };
  return { controller, session, select: (index, keys = {}) => controller.select(session, `/file-${index}`, "", { ctrlKey: false, shiftKey: false, metaKey: false, ...keys }) };
}
test("Ctrl toggles without discarding other files", () => {
  const { session, select } = fixture();
  select(1); select(3, { ctrlKey: true });
  assert.deepEqual([...session.selectedPaths], ["/file-1", "/file-3"]);
  select(1, { ctrlKey: true });
  assert.deepEqual([...session.selectedPaths], ["/file-3"]);
  assert.equal(session.selected, "/file-3");
  select(3, { ctrlKey: true }); assert.equal(session.selected, "");
});
test("Shift ranges in either direction and Ctrl+Shift adds to the set", () => {
  const { session, select } = fixture();
  select(4); select(1, { shiftKey: true });
  assert.deepEqual([...session.selectedPaths], [1, 2, 3, 4].map(index => `/file-${index}`));
  select(0, { ctrlKey: true }); select(2, { ctrlKey: true, shiftKey: true });
  assert.equal(session.selectedPaths.size, 5);
  assert.equal(session.selectedPaths.has("/file-0"), true);
});
test("parent navigation and blank selection never become downloadable group members", () => {
  const { controller, session, select } = fixture();
  select(1); select(2, { ctrlKey: true });
  controller.select(session, "/", "parent");
  assert.equal(session.selectedPaths.size, 0);
  assert.equal(controller.actionTarget(session).entry, undefined);
  controller.select(session, ""); assert.equal(session.selectedNavigation, "");
});
test("rename/download require one item, while group delete targets all selected items", () => {
  const { controller, session, select } = fixture();
  select(1); select(3, { ctrlKey: true });
  for (const action of ["rename", "download"]) assert.equal(controller.actionDisabled(session, action), true);
  assert.equal(controller.actionDisabled(session, "delete"), false);
  assert.deepEqual(controller.actionTarget(session).entries.map(entry => entry.path), ["/file-1", "/file-3"]);
  assert.equal(controller.actionDisabled(session, "download", { directory: "/", entry: session.entries[1] }), false);
});
test("Shift uses the entire directory rather than the visible virtualized rows", () => {
  const { session, select } = fixture(5000);
  select(2); select(4999, { shiftKey: true });
  assert.equal(session.selectedPaths.size, 4998);
});

test("ended transfers render determinate progress even when backend supplied no percentage", () => {
  const { controller, session } = fixture();
  for (const phase of ["cancelled", "failed", "completed"]) {
    session.transfer = { id: "old", phase, percent: null, message: "ended", bytes: 0, elapsed_ms: 0 };
    const html = controller.renderTransfer(session);
    assert.match(html, /<progress value="(?:0|100)" max="100"/);
    assert.doesNotMatch(html, /cancel-transfer/);
  }
  session.transfer.phase = "running";
  session.cancelling = true;
  assert.match(controller.renderTransfer(session), /<progress value="0" max="100"/);
});

test("transfer details omit elapsed seconds both on initial render and live progress", () => {
  const { controller, session } = fixture();
  session.transfer = { id: "progress", device_id: "A", phase: "running", percent: 25, bytes: 1024, message: "传输中", elapsed_ms: 54000 };
  const html = controller.renderTransfer(session);
  assert.match(html, /data-files-transfer-detail>25% · 1\.0 KB<\/span>/);
  assert.doesNotMatch(html, /秒/);
  const detail = { textContent: "" };
  const originalDocument = globalThis.document;
  globalThis.document = { querySelector: selector => selector === "[data-files-transfer-detail]" ? detail : null };
  controller.sessions = new Map([["device-files:A", session]]);
  controller.hooks.activeId = () => session.id;
  try {
    controller.onProgress({ ...session.transfer, percent: 50, bytes: 2048, elapsed_ms: 70000 });
    assert.equal(detail.textContent, "50% · 2.0 KB");
  } finally {
    if (originalDocument === undefined) delete globalThis.document;
    else globalThis.document = originalDocument;
  }
});

test("three-second notice timers are isolated by transfer ID and batch lifetime", context => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { controller, session } = fixture();
  controller.transferTimers = new Map();
  controller.update = () => {};
  session.transfer = { id: "old", phase: "completed" };
  controller.scheduleTransferDismiss(session);
  context.mock.timers.tick(2999); assert.ok(session.transfer);
  context.mock.timers.tick(1); assert.equal(session.transfer, null);
  session.transfer = { id: "old-again", phase: "failed" }; controller.scheduleTransferDismiss(session);
  context.mock.timers.tick(2000);
  session.transfer = { id: "new", phase: "running" }; controller.clearTransferTimer(session);
  context.mock.timers.tick(3000); assert.equal(session.transfer.id, "new");
  session.transfer.phase = "completed"; session.batch = { cancelled: false }; controller.scheduleTransferDismiss(session);
  context.mock.timers.tick(4000); assert.equal(session.transfer.id, "new");
  session.batch = null; controller.scheduleTransferDismiss(session);
  context.mock.timers.tick(3000); assert.equal(session.transfer, null);
});
