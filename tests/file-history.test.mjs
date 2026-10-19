// node --experimental-transform-types --test tests/file-history.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { DeviceFiles } from "../src/device-files.ts";

function fixture() {
  const controller = Object.create(DeviceFiles.prototype);
  const session = { id: "files:A", deviceId: "A", path: "/", draftPath: "/", history: [], historyIndex: -1, entries: [],
    selected: "", selectedNavigation: "", selectedPaths: new Set(), selectionAnchor: "", loading: false, busy: false,
    syncing: false, transfer: null, batch: null, dragging: false, request: 0, cacheVersion: 0, scroll: 0,
    directories: new Map(), expanded: new Set(), treeErrors: new Map() };
  controller.sessions = new Map([[session.id, session]]);
  controller.hooks = { activeId: () => session.id, online: () => true };
  controller.update = () => {};
  controller.closeContextMenu = () => {};
  controller.endMarquee = () => {};
  controller.saveHistoryScroll = current => {
    const entry = current.history[current.historyIndex];
    if (entry?.path === current.path) entry.scroll = current.scroll;
  };
  controller.readDirectory = async (_session, path) => ({ path, entries: [] });
  return { controller, session, navigate: (path, force = false) => controller.navigate(session, path, force), travel: direction => controller.travelHistory(session, direction) };
}

test("only successful directory changes enter history; refresh and failures preserve forward entries", async () => {
  const { controller, session, navigate, travel } = fixture();
  await navigate("/"); await navigate("/sdcard"); await navigate("/system");
  await travel(-1);
  await navigate("/sdcard", true); await navigate("/sdcard/.");
  assert.deepEqual(session.history.map(item => item.path), ["/", "/sdcard", "/system"]);
  assert.equal(session.historyIndex, 1);
  controller.readDirectory = async () => { throw new Error("permission denied"); };
  await navigate("/denied");
  assert.equal(session.path, "/sdcard"); assert.equal(session.historyIndex, 1);
  assert.equal(session.history.length, 3);
  controller.readDirectory = async (_session, path) => ({ path, entries: [] });
  await travel(1); assert.equal(session.path, "/system");
});

test("back/forward restore saved scroll and a failed traversal doesn't move the cursor", async () => {
  const { controller, session, navigate, travel } = fixture();
  await navigate("/"); await navigate("/large"); session.scroll = 3200;
  await navigate("/sdcard"); session.scroll = 96;
  controller.readDirectory = async () => { throw new Error("disconnected"); };
  await travel(-1);
  assert.equal(session.path, "/sdcard"); assert.equal(session.historyIndex, 2); assert.equal(session.scroll, 96);
  controller.readDirectory = async (_session, path) => ({ path, entries: [] });
  await travel(-1); assert.equal(session.path, "/large"); assert.equal(session.scroll, 3200);
  await travel(1); assert.equal(session.path, "/sdcard"); assert.equal(session.scroll, 96);
  await travel(1); assert.equal(session.historyIndex, 2);
});

test("a new destination after Back truncates the old forward branch and history is bounded", async () => {
  const { session, navigate, travel } = fixture();
  await navigate("/"); await navigate("/a"); await navigate("/b"); await travel(-1); await navigate("/new");
  assert.deepEqual(session.history.map(item => item.path), ["/", "/a", "/new"]);
  await travel(1); assert.equal(session.path, "/new");
  for (let index = 0; index < 150; ++index) await navigate(`/dir-${index}`);
  assert.equal(session.history.length, 128); assert.equal(session.historyIndex, 127);
  assert.equal(session.history[0].path, "/dir-22");
});

test("superseded asynchronous reads never enter history or change the traversal cursor", async () => {
  const { controller, session, navigate } = fixture();
  await navigate("/");
  let resolveOld;
  controller.readDirectory = (_session, path) => path === "/slow" ? new Promise(resolve => { resolveOld = resolve; }) : Promise.resolve({ path, entries: [] });
  const old = navigate("/slow");
  await navigate("/new");
  resolveOld({ path: "/slow", entries: [] }); await old;
  assert.equal(session.path, "/new"); assert.deepEqual(session.history.map(item => item.path), ["/", "/new"]);
});

test("each device tab owns its history and inactive tabs cannot be navigated by side buttons", async () => {
  const { controller, session, navigate } = fixture();
  await navigate("/"); await navigate("/sdcard");
  const other = fixture().session; other.id = "files:B"; other.deviceId = "B";
  controller.sessions.set(other.id, other);
  controller.hooks.activeId = () => other.id;
  await controller.navigate(other, "/"); await controller.navigate(other, "/data");
  await controller.travelHistory(session, -1); assert.equal(session.path, "/sdcard");
  await controller.travelHistory(other, -1); assert.equal(other.path, "/");
  assert.deepEqual(session.history.map(item => item.path), ["/", "/sdcard"]);
});

test("loading, mutations, modal confirmation, offline state and native dragging block traversal", async () => {
  const { controller, session, navigate, travel } = fixture();
  await navigate("/"); await navigate("/sdcard");
  for (const field of ["loading", "busy", "dragging"]) {
    session[field] = true; await travel(-1); session[field] = false;
    assert.equal(session.path, "/sdcard");
  }
  controller.dialog = {}; await travel(-1); controller.dialog = null;
  controller.hooks.online = () => false; await travel(-1);
  assert.equal(session.path, "/sdcard"); assert.equal(session.historyIndex, 1);
});

test("mouse side-button sequence is consumed but only mouseup navigates once", () => {
  const { controller } = fixture();
  const directions = []; let prevented = 0;
  controller.travelHistory = (_session, direction) => { directions.push(direction); };
  for (const type of ["mousedown", "mouseup", "auxclick"]) controller.onHistoryMouse({ type, button: 3, preventDefault: () => ++prevented, stopPropagation() {} });
  assert.deepEqual(directions, [-1]); assert.equal(prevented, 3);
  controller.onHistoryMouse({ type: "mouseup", button: 4, preventDefault: () => ++prevented, stopPropagation() {} });
  assert.deepEqual(directions, [-1, 1]);
  for (const button of [0, 1, 2]) controller.onHistoryMouse({ type: "mouseup", button, preventDefault: () => ++prevented, stopPropagation() {} });
  controller.hooks.activeId = () => "serial";
  controller.onHistoryMouse({ type: "mouseup", button: 3, preventDefault: () => ++prevented, stopPropagation() {} });
  assert.equal(prevented, 4); assert.deepEqual(directions, [-1, 1]);
});
