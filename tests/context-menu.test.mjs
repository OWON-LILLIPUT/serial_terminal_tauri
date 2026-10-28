// node --test tests/context-menu.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import ts from "typescript";

function listener(file, receiver) {
  const source = ts.createSourceFile(file, readFileSync(new URL(`../src/${file}`, import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
  let callback;
  function visit(node) {
    if (ts.isCallExpression(node) && node.arguments[0]?.text === "contextmenu" && receiver(node.expression.getText(source))) callback = node.arguments[1];
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.ok(callback, `${file} contextmenu listener exists`);
  return ts.transpile(`globalThis.handler = ${callback.getText(source)};`, { target: ts.ScriptTarget.ES2022 });
}

const globalListener = listener("main.ts", receiver => receiver === "document.addEventListener");
const fileListener = listener("device-files.ts", receiver => receiver.includes('querySelector(".device-files")'));

class Target {
  constructor(...regions) { this.regions = new Set(regions); }
  closest(selector) { return selector.split(",").some(region => this.regions.has(region.trim())) ? this : null; }
}

function fixture(...regions) {
  let opened = 0;
  const context = { Element: Target, dialog: null, session: {}, select: () => {}, actionTarget: () => ({}), openContextMenu: () => { ++opened; } };
  runInNewContext(fileListener, context);
  const event = { target: new Target(...regions), clientX: 10, clientY: 20, prevented: false, stopped: false,
    preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; } };
  return { context, event, run: () => context.handler(event), opened: () => opened };
}

test("the WebView default menu is suppressed without blocking custom handlers", () => {
  const context = {};
  runInNewContext(globalListener, context);
  let prevented = false;
  context.handler({ preventDefault: () => { prevented = true; }, stopPropagation: () => assert.fail("must not block supported menus") });
  assert.equal(prevented, true);
});

test("file list and directory tree retain their custom menu", () => {
  for (const region of [".device-files-list", ".device-files-tree"]) {
    const f = fixture(region);
    f.run();
    assert.equal(f.opened(), 1);
    assert.equal(f.event.prevented, true);
    assert.equal(f.event.stopped, true);
  }
});

test("toolbar, footer, title, sidebar and text fields do not open a file menu", () => {
  for (const regions of [[".device-files-toolbar"], [".device-files-footer"], [".topbar"], [".sidebar"], [".device-files"], [".device-files-list", "input"], [".device-files-tree", "textarea"], [".device-files-list", "[contenteditable]"]]) {
    const f = fixture(...regions);
    f.run();
    assert.equal(f.opened(), 0, regions.join());
  }
});

test("modal confirmation and non-element targets do not open a file menu", () => {
  const f = fixture(".device-files-list");
  f.context.dialog = {};
  f.run();
  f.context.dialog = null;
  f.event.target = null;
  f.run();
  assert.equal(f.opened(), 0);
});
