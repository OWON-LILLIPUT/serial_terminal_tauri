// node --test tests/remote-adb-input.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import ts from "typescript";

// Execute the production helpers without starting the UI or invoking ADB.
const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const names = ["normalizeRemoteAdbAddress", "selectedAdbDeviceInfo", "hasRemoteAdbDevice", "isRemoteAdbInputActive", "syncRemoteAdbInputWithSelectedDevice"];
const helpers = names.map(name => {
  const declaration = source.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.ok(declaration, `missing production helper: ${name}`);
  return declaration.getText(source);
}).join("\n");
const script = ts.transpile(helpers, { target: ts.ScriptTarget.ES2022 });
const remote = { id: "192.168.30.139:5555", is_remote: true, state: "device" };
const local = { id: "USB-A", is_remote: false, state: "device" };

function fixture(input, devices = [remote, local], selected = local.id, focused = false) {
  const state = { remoteAdbInput: input, selectedRemoteAdb: remote.id, selectedAdbDevice: selected, adbDevices: devices };
  const context = { state, defaultRemoteAdbAddress: "192.168.30.1:5555", document: { activeElement: focused ? { closest: () => ({}) } : null } };
  runInNewContext(script, context);
  return { state, sync: context.syncRemoteAdbInputWithSelectedDevice };
}

test("switching to USB keeps the current IP and port while remote devices exist", () => {
  for (const input of [remote.id, "192.168.30.140:6000", "192.168..140:6000"]) {
    const { state, sync } = fixture(input);
    sync(true);
    assert.equal(state.remoteAdbInput, input);
    assert.equal(state.selectedRemoteAdb, "");
  }
});

test("switching between local devices does not reset a custom IP to the default", () => {
  const { state, sync } = fixture("10.0.0.42:6789", [local, { ...local, id: "USB-B" }]);
  sync(true);
  state.selectedAdbDevice = "USB-B";
  sync(true);
  sync(false);
  assert.equal(state.remoteAdbInput, "10.0.0.42:6789");
});

test("an empty address still gets the default when no remote device exists", () => {
  const { state, sync } = fixture("", [local]);
  sync(true);
  assert.equal(state.remoteAdbInput, "192.168.30.1:5555");
});

test("explicit remote selection still fills its own address", () => {
  const { state, sync } = fixture("10.0.0.42:6789", [remote, local], remote.id);
  sync(true);
  assert.equal(state.remoteAdbInput, remote.id);
  assert.equal(state.selectedRemoteAdb, remote.id);
});

test("background refresh does not overwrite the currently focused address", () => {
  const { state, sync } = fixture("192.168.30.140:6000", [remote, local], remote.id, true);
  sync(false);
  assert.equal(state.remoteAdbInput, "192.168.30.140:6000");
});
