// Manual/automated browser fixture: npm run dev -- --port 1422,
// then open /tests/device-files-ui.html. All device/local filesystem commands are mocked.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { DeviceFileEntry } from "../src/device-files";

type EntryMap = Map<string, DeviceFileEntry[]>;
const calls: { command: string; args: any }[] = [];
const entry = (parent: string, name: string, dir = false): DeviceFileEntry => ({ name, path: `${parent === "/" ? "" : parent}/${name}`, is_dir: dir, kind: dir ? "directory" : "file", size: 24576, modified: 1700000000, permissions: dir ? "755" : "644" });
const deviceA: EntryMap = new Map([
  ["/", [entry("/", "sdcard", true), entry("/", "data", true), entry("/", "system", true), entry("/", "large", true)]],
  ["/sdcard", [entry("/sdcard", "Download", true), entry("/sdcard", ".hidden-dir", true), entry("/sdcard", ".hidden"), entry("/sdcard", "空格 ' $文件.txt"), entry("/sdcard", "<img onerror=alert(1)>.txt"), entry("/sdcard", "upload.txt")]],
  ["/sdcard/.hidden-dir", [entry("/sdcard/.hidden-dir", "nested", true)]],
  ["/sdcard/.hidden-dir/nested", []],
  ["/sdcard/Download", []],
  ["/system", []],
  ["/large", Array.from({ length: 5000 }, (_, index) => ({ ...entry("/large", `${index < 60 ? "长文件名测试_".repeat(8) : ""}file-${String(index).padStart(4, "0")}.txt`, index < 60 && index % 2 === 0), size: index < 60 ? 9000000000 : 1 }))],
]);
const deviceB: EntryMap = new Map([["/", [entry("/", "different-device.txt")]]]);
const files = new Map([["TEST-A", deviceA], ["TEST-B", deviceB]]);
const transfers = new Map<string, { request: any; cancel: boolean }>();
const nativeDrags = new Map<string, { cancel: boolean }>();
let deviceAOnline = true;
let failNextSync = false;
let failDrag = false;
let failNextUpload = false;
let failDeletePath = "";
const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
localStorage.setItem("serial-terminal-settings-v1", JSON.stringify({ selectedAdbDevice: "TEST-A" }));
mockWindows("main");
mockIPC(async (command, args: any = {}) => {
  calls.push({ command, args: structuredClone(args) });
  if (command === "list_fonts") return ["Consolas", "Cascadia Mono"];
  if (command === "list_ports") return ["COM5"];
  if (command === "list_adb_state") return { devices: ["TEST-A", "TEST-B"].map(id => ({ id, state: id === "TEST-A" && !deviceAOnline ? "offline" : "device", is_remote: false })), scrcpy_devices: [] };
  if (command === "list_scrcpy_devices") return [];
  if (command === "get_adb_server_status") return { phase: "ready", message: "", revision: 1 };
  if (command.includes("is_maximized")) return false;
  if (command === "list_device_directory") {
    await delay(args.path === "/system" ? 150 : 10);
    const entries = files.get(args.deviceId)?.get(args.path);
    if (!entries) throw new Error("Permission denied: " + args.path);
    return { path: args.path, entries: structuredClone(entries) };
  }
  if (command === "sync_device_files") {
    const fail = failNextSync; failNextSync = false;
    await delay(500);
    if (fail) throw new Error("simulated sync failure");
    return null;
  }
  if (command === "mutate_device_file") {
    if (args.action === "delete" && args.path === failDeletePath) { failDeletePath = ""; throw new Error("simulated delete denied"); }
    const map = files.get(args.deviceId)!;
    if (args.action === "mkdir") {
      map.get(args.path)!.push(entry(args.path, args.name, true));
      map.set(`${args.path}/${args.name}`, []);
    } else {
      const parent = args.path.slice(0, args.path.lastIndexOf("/")) || "/";
      const entries = map.get(parent)!;
      const index = entries.findIndex(entry => entry.path === args.path);
      if (args.action === "rename") entries[index] = { ...entries[index], name: args.name, path: `${parent}/${args.name}` };
      else if (args.action === "delete") { if (!args.confirmed) throw new Error("confirmation required"); entries.splice(index, 1); }
    }
    return null;
  }
  if (command === "choose_device_local_path") {
    if (args.mode === "upload-directory") return null; // Simulate cancelling the native folder chooser.
    if (args.mode.startsWith("upload")) return { path: "C:\\mock\\upload.txt", name: "upload.txt", exists: true, is_dir: false, size: 100000 };
    return { path: "C:\\mock\\saved.txt", name: "saved.txt", exists: true, is_dir: false, size: 10 };
  }
  if (command === "inspect_device_upload_paths") {
    await delay(20);
    if (!args.paths.length || args.paths.some((path: string) => path.endsWith("folder"))) throw new Error("拖入上传暂只支持普通文件");
    return args.paths.map((path: string) => ({ path, name: path.split(/[/\\]/).at(-1), exists: true, is_dir: false, size: 12345 }));
  }
  if (command === "drag_device_file") {
    const record = { cancel: false }; nativeDrags.set(args.id, record);
    const fail = failDrag; failDrag = false;
    try {
      await delay(150);
      if (record.cancel || fail) throw new Error("已取消传输：没有复制到目标文件夹");
      await emit("device-file-transfer", { id: args.id, device_id: args.deviceId, phase: "dragging", message: "已投放，等待目标读取文件…", percent: null, bytes: 0, total: null, elapsed_ms: 150 });
      await delay(100);
      if (record.cancel) throw new Error("已取消传输");
      await emit("device-file-transfer", { id: args.id, device_id: args.deviceId, phase: "running", message: "目标已接收，模拟下载中", percent: 30, bytes: 3, total: 10, elapsed_ms: 250 });
      await delay(100);
      if (record.cancel) throw new Error("已取消传输");
      return null;
    } finally { nativeDrags.delete(args.id); }
  }
  if (command === "transfer_device_file") {
    if (failNextUpload) { failNextUpload = false; throw new Error("simulated upload failure"); }
    const record = { request: args.request, cancel: false };
    transfers.set(record.request.id, record);
    for (let percent = 10; percent <= 100; percent += 10) {
      await delay(80);
      if (record.cancel) break;
      await emit("device-file-transfer", { id: record.request.id, device_id: record.request.device_id, phase: "running", message: "模拟传输中", percent, bytes: percent * 1000, total: 100000, elapsed_ms: percent * 8 });
    }
    if (!record.cancel && record.request.direction === "upload" && record.request.local_path.includes("drag-")) {
      const remote = record.request.remote_path as string;
      const parent = remote.slice(0, remote.lastIndexOf("/")) || "/";
      const name = remote.slice(remote.lastIndexOf("/") + 1);
      const entries = files.get(record.request.device_id)!.get(parent)!;
      if (!entries.some(item => item.name === name)) entries.push(entry(parent, name));
    }
    await emit("device-file-transfer", { id: record.request.id, device_id: record.request.device_id, phase: record.cancel ? "cancelled" : "completed", message: record.cancel ? "已取消传输" : "传输完成", percent: record.cancel ? null : 100, bytes: 100000, total: 100000, elapsed_ms: 800 });
    transfers.delete(record.request.id);
    if (record.cancel) throw new Error("已取消传输");
    return null;
  }
  if (command === "cancel_device_transfer") {
    const transfer = transfers.get(args.id); if (transfer) transfer.cancel = true;
    const drag = nativeDrags.get(args.id); if (drag) drag.cancel = true;
    return null;
  }
  return null;
}, { shouldMockEvents: true });
await import("../src/main");

function assert(condition: unknown, label: string): asserts condition { if (!condition) throw new Error(label); }
function click(selector: string): void {
  const element = document.querySelector<HTMLElement>(selector);
  assert(element, `missing ${selector}`);
  assert(!(element instanceof HTMLButtonElement) || !element.disabled, `disabled ${selector}`);
  element.click();
}
async function waitFor(test: () => boolean, label: string): Promise<void> {
  for (let tries = 0; tries < 100; ++tries) { if (test()) return; await delay(20); }
  throw new Error("timeout: " + label);
}
const actions = (action: string) => `[data-files-action="${action}"]`;
const menuActions = (action: string) => `[data-files-menu-action="${action}"]`;
function rightClick(selector: string, x = window.innerWidth - 2, y = window.innerHeight - 2): void {
  const target = document.querySelector<HTMLElement>(selector)!;
  assert(target, `missing right-click target ${selector}`);
  target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: x, clientY: y }));
}
const mutations = () => calls.filter(call => call.command === "mutate_device_file");
const transferCalls = () => calls.filter(call => call.command === "transfer_device_file");
const reads = (path: string) => calls.filter(call => call.command === "list_device_directory" && call.args.deviceId === "TEST-A" && call.args.path === path).length;
async function navigate(path: string): Promise<void> {
  await waitFor(() => document.querySelector<HTMLButtonElement>(actions("refresh"))?.disabled === false, `ready to navigate ${path}`);
  const input = document.querySelector<HTMLInputElement>("#device-files-path")!;
  input.value = path; input.dispatchEvent(new Event("input", { bubbles: true }));
  document.querySelector<HTMLFormElement>("#device-files-path-form")!.requestSubmit();
  await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === path
    && !document.querySelector<HTMLButtonElement>(actions("refresh"))!.disabled, `navigate ${path}`);
}
function submitName(name: string): void {
  const input = document.querySelector<HTMLInputElement>("#device-files-dialog-input")!;
  input.value = name;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
}

document.querySelector("#run-ui-tests")!.addEventListener("click", async () => {
  const result = document.querySelector("#ui-test-result")!;
  const run = document.querySelector<HTMLButtonElement>("#run-ui-tests")!;
  run.disabled = true;
  let passed = 0;
  try {
    await waitFor(() => !document.querySelector<HTMLButtonElement>("#device-files-open")!.disabled, "devices loaded");
    const rootButton = document.querySelector("#adb-root")!.getBoundingClientRect();
    const filesButton = document.querySelector("#device-files-open")!.getBoundingClientRect();
    assert(Math.abs(rootButton.top - filesButton.top) < 1 && Math.abs(rootButton.height - filesButton.height) < 1, "ROOT/browser buttons align on same row"); ++passed;
    click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard"]'), "root listing");
    assert(!document.querySelector("#rx-stat"), "file tab hides serial statistics");
    assert(document.querySelectorAll('[role="tab"][aria-selected="true"]').length === 1, "only file tab is selected"); ++passed;
    const row = document.querySelector<HTMLElement>('[data-file-path="/sdcard"]')!;
    row.click(); row.click(); row.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/Download"]'), "double click opens directory");
    assert(document.querySelector<HTMLInputElement>("#device-files-path")!.value === "/sdcard", "navigation updates input");
    assert(!document.querySelector(".device-files-table img"), "HTML filename is escaped"); ++passed;
    assert(document.querySelector('[data-files-expand="/sdcard"]')?.getAttribute("aria-expanded") === "false", "list navigation doesn't expand tree");
    assert(!document.querySelector('[data-files-directory="/sdcard/.hidden-dir"]'), "list navigation keeps tree children hidden");
    click('[data-files-expand="/sdcard"]');
    await waitFor(() => !!document.querySelector('[data-files-directory="/sdcard/.hidden-dir"]'), "hidden directory appears in tree");
    assert(!!document.querySelector('[data-file-path="/sdcard/.hidden-dir"]'), "hidden directory appears in list");
    assert(document.querySelectorAll('[data-file-navigation="parent"]').length === 1 && !document.querySelector('[data-file-navigation="current"]'), "only parent navigation is rendered");
    const dotdot = '[data-file-navigation="parent"]';
    const beforeDotWrites = mutations().length;
    click(dotdot);
    document.querySelector(".device-files-list")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    assert(mutations().length === beforeDotWrites && !document.querySelector("#device-files-dialog"), "parent navigation can't be deleted");
    rightClick(dotdot);
    assert(["download", "rename", "delete"].every(action => document.querySelector<HTMLButtonElement>(menuActions(action))!.disabled), "dot entries are navigation only");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    click(dotdot); document.querySelector(".device-files-list")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === "/", "dotdot navigates to parent with Enter");
    click(dotdot);
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 1, "root parent navigation selects one row");
    document.querySelector(dotdot)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await delay(40);
    assert(document.querySelector<HTMLElement>(".device-files")!.dataset.fileDirectory === "/", "root parent navigation stays at root");
    await navigate("/sdcard"); ++passed;
    const hiddenPath = "/sdcard/.hidden-dir";
    click(`[data-files-directory="${hiddenPath}"]`);
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === hiddenPath, "name click navigates right panel");
    assert(document.querySelector(`[data-files-expand="${hiddenPath}"]`)?.getAttribute("aria-expanded") === "false" && !document.querySelector(`[data-files-directory="${hiddenPath}/nested"]`), "name navigation doesn't expand children");
    assert(!!document.querySelector(`[data-file-path="${hiddenPath}/nested"]`), "right panel shows folder contents while tree is collapsed");
    const hiddenReads = reads(hiddenPath);
    click(`[data-files-directory="${hiddenPath}"]`);
    assert(document.querySelector(`[data-files-expand="${hiddenPath}"]`)?.getAttribute("aria-expanded") === "false" && reads(hiddenPath) === hiddenReads, "repeated collapsed name doesn't expand or reread");
    click(`[data-files-expand="${hiddenPath}"]`);
    await waitFor(() => !!document.querySelector(`[data-files-directory="${hiddenPath}/nested"]`), "arrow expands cached children");
    assert(document.querySelector<HTMLElement>(".device-files")!.dataset.fileDirectory === hiddenPath, "arrow doesn't navigate right panel");
    click(`[data-files-directory="${hiddenPath}"]`);
    await waitFor(() => document.querySelector(`[data-files-expand="${hiddenPath}"]`)?.getAttribute("aria-expanded") === "false", "current expanded name collapses");
    assert(!document.querySelector(`[data-files-directory="${hiddenPath}/nested"]`) && reads(hiddenPath) === hiddenReads, "collapse doesn't reread or retain children");
    assert(document.querySelector<HTMLElement>(".device-files")!.dataset.fileDirectory === hiddenPath, "collapse preserves current file list");
    click(`[data-files-expand="${hiddenPath}"]`);
    await waitFor(() => !!document.querySelector(`[data-files-directory="${hiddenPath}/nested"]`), "only arrow reopens children");
    await navigate("/sdcard");
    click(`[data-files-directory="${hiddenPath}"]`);
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === hiddenPath, "noncurrent expanded name navigates");
    assert(document.querySelector(`[data-files-expand="${hiddenPath}"]`)?.getAttribute("aria-expanded") === "false" && !document.querySelector(`[data-files-directory="${hiddenPath}/nested"]`), "noncurrent expanded name also collapses");
    await navigate("/sdcard"); ++passed;
    click('[data-files-directory="/sdcard"]');
    await waitFor(() => document.querySelector('[data-files-expand="/sdcard"]')?.getAttribute("aria-expanded") === "false", "collapse ancestor before deep navigation");
    await navigate(`${hiddenPath}/nested`);
    assert(document.querySelector('[data-files-expand="/sdcard"]')?.getAttribute("aria-expanded") === "false" && !document.querySelector(`[data-files-directory="${hiddenPath}"]`), "typed deep path doesn't expand ancestors");
    const beforeCollapsedRefresh = document.querySelector(".device-files");
    click(actions("refresh"));
    await waitFor(() => document.querySelector(".device-files") !== beforeCollapsedRefresh && !document.querySelector<HTMLButtonElement>(actions("refresh"))!.disabled, "refresh deep path completes");
    assert(document.querySelector('[data-files-expand="/sdcard"]')?.getAttribute("aria-expanded") === "false", "refresh doesn't reopen collapsed ancestor");
    await navigate("/sdcard"); ++passed;
    click('[data-files-expand="/sdcard"]');
    await waitFor(() => !!document.querySelector(`[data-files-directory="${hiddenPath}"]`), "restore explicitly expanded ancestor");
    const sidebarButton = document.querySelector("#adb-root");
    const sdcardReads = reads("/sdcard");
    click(actions("up"));
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === "/", "up directory completes");
    await navigate("/sdcard");
    assert(reads("/sdcard") === sdcardReads, "revisiting directory uses session cache");
    assert(document.querySelector("#adb-root") === sidebarButton, "directory navigation preserves sidebar DOM");
    const beforeRefreshPanel = document.querySelector(".device-files");
    click(actions("refresh"));
    await waitFor(() => reads("/sdcard") === sdcardReads + 1 && document.querySelector(".device-files") !== beforeRefreshPanel && !document.querySelector<HTMLButtonElement>(actions("refresh"))!.disabled, "manual refresh bypasses cache"); ++passed;
    const systemReads = reads("/system");
    click('[data-files-expand="/system"]'); click('[data-files-directory="/system"]');
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === "/system", "shared tree/list request");
    assert(reads("/system") === systemReads + 1, "tree and list coalesce a pending directory read");
    assert(document.querySelector('[data-files-expand="/system"]')?.getAttribute("aria-expanded") === "false", "late arrow load doesn't undo name collapse"); ++passed;
    await navigate("/large");
    const largeList = document.querySelector<HTMLElement>(".device-files-list")!;
    assert(document.querySelectorAll("[data-file-path]").length < 100, "5000 entries render only viewport rows");
    // Collapsed thead borders contribute half a pixel to the first row only.
    assert([...document.querySelectorAll(".device-files-table tr[data-file-path]")].every(row => Math.abs(row.getBoundingClientRect().height - 32) < 0.6), "folder and navigation icons keep virtual row height");
    assert(Math.abs(largeList.scrollHeight - (5001 * 32 + 32)) < 10, "virtual row height includes parent navigation");
    const columnPositions = () => [...document.querySelectorAll(".device-files-table th")].map(cell => cell.getBoundingClientRect().left);
    const columnsBefore = columnPositions();
    largeList.scrollTop = largeList.scrollHeight; largeList.dispatchEvent(new Event("scroll"));
    await waitFor(() => !!document.querySelector('[data-file-path="/large/file-4999.txt"]'), "virtual list tail visible");
    assert(columnPositions().every((left, index) => Math.abs(left - columnsBefore[index]) < 0.5), "long-to-short rows keep header columns fixed");
    largeList.scrollTop = largeList.scrollHeight / 2; largeList.dispatchEvent(new Event("scroll"));
    await delay(40);
    assert(columnPositions().every((left, index) => Math.abs(left - columnsBefore[index]) < 0.5), "middle scroll keeps header columns fixed");
    largeList.scrollTop = 0; largeList.dispatchEvent(new Event("scroll")); await delay(40);
    assert(columnPositions().every((left, index) => Math.abs(left - columnsBefore[index]) < 0.5), "upward scroll keeps header columns fixed");
    largeList.scrollTop = largeList.scrollHeight; largeList.dispatchEvent(new Event("scroll"));
    await waitFor(() => !!document.querySelector('[data-file-path="/large/file-4999.txt"]'), "tail restored after scroll geometry tests");
    click('[data-file-path="/large/file-4999.txt"]'); click(actions("rename"));
    assert(document.querySelector<HTMLInputElement>("#device-files-dialog-input")!.value === "file-4999.txt", "virtual tail action targets exact file");
    click("#device-files-dialog-cancel");
    await navigate("/sdcard");
    assert(document.querySelector<HTMLElement>(".device-files-list")!.scrollTop === 0, "new directory resets prior scroll");
    assert(!!document.querySelector('[data-file-path="/sdcard/Download"]'), "navigation from large directory isn't blank"); ++passed;
    assert(parseFloat(getComputedStyle(document.querySelector(".device-files-tree-toggle")!).fontSize) >= 20, "tree folder arrows are larger");
    assert(parseFloat(getComputedStyle(document.querySelector(".device-file-icon.folder")!).fontSize) >= 20, "list folder arrows are larger");
    const contextSidebar = document.querySelector("#adb-root");
    rightClick('[data-file-path="/sdcard/.hidden"]');
    const menu = document.querySelector<HTMLElement>("#device-files-context-menu")!;
    assert(menu?.parentElement === document.body && menu.classList.contains("terminal-context-menu"), "menu uses terminal theme in unclipped portal");
    assert(document.querySelector('[data-file-path="/sdcard/.hidden"]')!.getAttribute("aria-selected") === "true", "right click selects target row");
    assert(document.querySelector("#adb-root") === contextSidebar, "opening menu doesn't rebuild sidebar");
    const expected = ["up", "go", "refresh", "upload-file", "upload-directory", "download", "mkdir", "rename", "delete", "sync"];
    assert(expected.every(action => menu.querySelector(menuActions(action))) && menu.querySelectorAll("button").length === expected.length, "all top actions are in menu");
    assert([...menu.querySelectorAll("button")].every(button => button.classList.contains("terminal-context-item")), "items share terminal menu style");
    const bounds = menu.getBoundingClientRect();
    assert(bounds.left >= 7 && bounds.top >= 7 && bounds.right <= window.innerWidth - 7 && bounds.bottom <= window.innerHeight - 7, "menu is clamped inside viewport");
    menu.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    assert((document.activeElement as HTMLElement).dataset.filesMenuAction === "sync", "menu keyboard End selects last action");
    menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true }));
    assert((document.activeElement as HTMLElement).dataset.filesMenuAction === "up", "menu keyboard Home selects first action");
    click(menuActions("rename"));
    assert(document.querySelector<HTMLInputElement>("#device-files-dialog-input")!.value === ".hidden", "menu rename uses right-click target");
    click("#device-files-dialog-cancel");
    rightClick(".device-files-list");
    assert(document.querySelector<HTMLButtonElement>(menuActions("download"))!.disabled && document.querySelector<HTMLButtonElement>(menuActions("rename"))!.disabled && document.querySelector<HTMLButtonElement>(menuActions("delete"))!.disabled, "blank menu disables file actions");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    assert(!document.querySelector("#device-files-context-menu"), "Escape closes file menu"); ++passed;
    const freshReads = reads("/sdcard");
    const beforeMenuRefreshPanel = document.querySelector(".device-files");
    rightClick(".device-files-list"); click(menuActions("refresh"));
    await waitFor(() => reads("/sdcard") === freshReads + 1 && document.querySelector(".device-files") !== beforeMenuRefreshPanel && !document.querySelector<HTMLButtonElement>(actions("refresh"))!.disabled, "menu refresh bypasses cache");
    await delay(30);
    rightClick(".device-files-list"); click(menuActions("mkdir")); submitName("menu-folder");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/menu-folder"]'), "menu creates directory");
    rightClick('[data-file-path="/sdcard/menu-folder"]'); click(menuActions("delete"));
    const menuDeleteCount = mutations().length;
    click("#device-files-dialog-cancel"); assert(mutations().length === menuDeleteCount, "menu delete still requires confirmation");
    rightClick('[data-file-path="/sdcard/menu-folder"]'); click(menuActions("delete"));
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => !document.querySelector('[data-file-path="/sdcard/menu-folder"]') && !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "confirmed menu delete completes"); ++passed;
    await waitFor(() => !!document.querySelector('[data-files-directory="/sdcard/Download"]'), "tree rebuilt after cache invalidation");
    rightClick('[data-files-directory="/sdcard/Download"]'); click(menuActions("mkdir")); submitName("tree-folder");
    const pendingTreeMutationPanel = document.querySelector(".device-files");
    await waitFor(() => mutations().some(call => call.args.path === "/sdcard/Download" && call.args.name === "tree-folder"), "tree menu targets selected directory");
    await waitFor(() => document.querySelector(".device-files") !== pendingTreeMutationPanel && !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled && !!document.querySelector('[data-files-directory="/sdcard/Download"]'), "tree mutation and ancestor rebuild complete");
    rightClick('[data-files-directory="/sdcard/Download"]'); click(menuActions("refresh"));
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/Download/tree-folder"]'), "tree menu refresh opens its directory");
    rightClick(".device-files-list"); click(menuActions("up"));
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === "/sdcard", "menu up opens parent");
    const gotoInput = document.querySelector<HTMLInputElement>("#device-files-path")!;
    gotoInput.value = "/system"; gotoInput.dispatchEvent(new Event("input", { bubbles: true }));
    rightClick(".device-files-list"); click(menuActions("go"));
    await waitFor(() => document.querySelector<HTMLElement>(".device-files")?.dataset.fileDirectory === "/system", "menu go uses typed path");
    await navigate("/sdcard"); ++passed;
    rightClick(".device-files-list"); document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    assert(!document.querySelector("#device-files-context-menu"), "outside click closes menu");
    rightClick(".device-files-list"); window.dispatchEvent(new Event("resize"));
    assert(!document.querySelector("#device-files-context-menu"), "resize closes menu");
    rightClick(".device-files-list"); document.querySelector(".device-files-list")!.dispatchEvent(new Event("scroll"));
    assert(!document.querySelector("#device-files-context-menu"), "scroll closes menu");
    rightClick(".device-files-list"); click(menuActions("upload-directory"));
    await waitFor(() => calls.some(call => call.command === "choose_device_local_path" && call.args.mode === "upload-directory"), "menu upload directory uses existing chooser");
    await delay(30);
    rightClick(".device-files-list"); click('[data-terminal-tab-id="serial"]');
    assert(!document.querySelector("#device-files-context-menu"), "tab switch closes menu");
    click('[data-files-tab="device-files:TEST-A"]'); ++passed;
    click(actions("mkdir")); const beforeCancel = mutations().length;
    click("#device-files-dialog-cancel"); assert(mutations().length === beforeCancel, "cancel mkdir has no writes");
    click(actions("mkdir")); submitName("测试目录");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/测试目录"]'), "mkdir refreshes list"); ++passed;
    assert(!document.querySelector("#device-files-dialog"), "accepted modal is removed after local panel update");
    click('[data-file-path="/sdcard/.hidden"]'); click(actions("rename")); submitName("new ' $name.txt");
    await waitFor(() => mutations().some(call => call.args.name === "new ' $name.txt"), "rename exact name");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/new \' $name.txt"]') && !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "rename complete"); ++passed;
    click('[data-file-path="/sdcard/upload.txt"]'); click(actions("delete"));
    const beforeDelete = mutations().length; click("#device-files-dialog-cancel"); assert(mutations().length === beforeDelete, "cancel delete no writes");
    click('[data-file-path="/sdcard/测试目录"]'); click(actions("delete"));
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => mutations().length > beforeDelete, "delete confirmation");
    assert(mutations().at(-1)!.args.confirmed === true, "delete sends explicit confirmation");
    await waitFor(() => !document.querySelector('[data-file-path="/sdcard/测试目录"]') && !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "delete complete"); ++passed;
    click(actions("upload-file"));
    await waitFor(() => !!document.querySelector("#device-files-dialog"), "upload overwrite dialog");
    const beforeTransfer = transferCalls().length; click("#device-files-dialog-cancel");
    assert(transferCalls().length === beforeTransfer, "cancel overwrite no transfer");
    rightClick(".device-files-list"); click(menuActions("upload-file")); await waitFor(() => !!document.querySelector("#device-files-dialog"), "second upload dialog");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => !!document.querySelector(".device-files-transfer progress"), "transfer started");
    const input = document.querySelector<HTMLInputElement>("#device-files-path")!; input.focus();
    await delay(200); assert(document.activeElement === input, "progress does not rebuild or steal input focus");
    click(actions("cancel-transfer"));
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent === "已取消传输", "cancel reaches terminal state"); ++passed;
    click('[data-picker-id="adbDevice"]'); click('[data-picker-value="TEST-B"]');
    click(actions("mkdir")); submitName("still-device-A");
    await waitFor(() => mutations().some(call => call.args.name === "still-device-A"), "device-bound operation");
    assert(mutations().at(-1)!.args.deviceId === "TEST-A", "changing device selector does not retarget tab");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/still-device-A"]') && !document.querySelector<HTMLButtonElement>(actions("sync"))!.disabled, "device A mutation complete");
    const syncCount = calls.filter(call => call.command === "sync_device_files").length;
    click(actions("sync")); click(actions("sync"));
    await waitFor(() => document.querySelector(actions("sync"))?.textContent === "正在同步…", "sync busy feedback");
    assert(document.querySelector<HTMLButtonElement>(actions("sync"))!.disabled, "pending sync prevents duplicate clicks");
    assert(calls.filter(call => call.command === "sync_device_files").length === syncCount + 1, "rapid clicks start only one sync");
    assert(calls.filter(call => call.command === "sync_device_files").at(-1)!.args.deviceId === "TEST-A", "sync uses tab device not selector");
    click('[data-terminal-tab-id="serial"]');
    assert(document.querySelector("#terminal-host .xterm"), "serial usable while sync runs");
    click('[data-files-tab="device-files:TEST-A"]');
    await waitFor(() => document.querySelector(".device-files-footer")?.textContent === "缓存已同步到设备存储", "sync completes with feedback"); ++passed;
    failNextSync = true; rightClick(".device-files-list"); click(menuActions("sync"));
    await waitFor(() => document.querySelector(".device-files-footer")?.textContent?.includes("同步失败：simulated sync failure") === true, "sync failure feedback");
    assert(!document.querySelector<HTMLButtonElement>(actions("sync"))!.disabled, "failed sync can retry");
    click(actions("sync"));
    await waitFor(() => document.querySelector(".device-files-footer")?.textContent === "缓存已同步到设备存储", "sync retry succeeds"); ++passed;
    click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/different-device.txt"]'), "device B independent tab");
    assert(document.querySelectorAll("[data-files-tab]").length === 2, "two file tabs"); ++passed;
    click('[data-files-tab="device-files:TEST-A"]');
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/upload.txt"]'), "return device A");
    rightClick('[data-file-path="/sdcard/upload.txt"]'); click(menuActions("download"));
    await waitFor(() => !!document.querySelector("#device-files-dialog"), "download overwrite confirmation");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => transferCalls().some(call => call.args.request.direction === "download"), "download started");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent === "传输完成", "download completes"); ++passed;
    const pathInput = document.querySelector<HTMLInputElement>("#device-files-path")!;
    pathInput.value = "/data"; pathInput.dispatchEvent(new Event("input", { bubbles: true }));
    document.querySelector<HTMLFormElement>("#device-files-path-form")!.requestSubmit();
    await waitFor(() => document.querySelector(".device-files-footer")?.textContent?.includes("Permission denied") === true, "permission error");
    assert(document.querySelector<HTMLInputElement>("#device-files-path")!.value === "/sdcard", "failed navigation retains actual directory path"); ++passed;
    deviceAOnline = false; click("#refresh-scrcpy-devices");
    await waitFor(() => !!document.querySelector(".device-files-notice"), "offline status updates file page");
    assert(document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "offline actions disabled");
    rightClick(".device-files-list");
    assert([...document.querySelectorAll<HTMLButtonElement>("[data-files-menu-action]")].every(button => button.disabled), "offline menu matches disabled top actions");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    deviceAOnline = true; click("#refresh-scrcpy-devices");
    await waitFor(() => !document.querySelector(".device-files-notice"), "reconnect status restored"); ++passed;
    click('[data-terminal-tab-id="serial"]');
    assert(document.querySelector("#terminal-host .xterm"), "serial terminal preserved after file tabs");
    assert(document.querySelector("#rx-stat"), "serial stats restored"); ++passed;
    click('[data-files-tab="device-files:TEST-A"]');
    click(actions("upload-file")); await waitFor(() => !!document.querySelector("#device-files-dialog"), "close transfer setup");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => !!document.querySelector(actions("cancel-transfer")), "close transfer started");
    click('[data-files-close="device-files:TEST-A"]');
    assert(document.querySelector("#device-files-dialog-title")!.textContent === "关闭文件标签？", "closing transfer asks confirmation");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => !document.querySelector('[data-files-tab="device-files:TEST-A"]'), "close after cancellation");
    assert(document.querySelector('[data-files-tab="device-files:TEST-B"]'), "other device tab remains"); ++passed;
    click('[data-files-tab="device-files:TEST-B"]');
    result.textContent = `PASS：${passed} 组隔离交互回归；没有操作真实文件`;
  } catch (error) { result.textContent = `FAIL（已过 ${passed} 组）：${String(error)}`; console.error(error); }
  finally { run.disabled = false; }
});

async function dropFiles(paths: string[], inside = true): Promise<void> {
  const rect = document.querySelector(".device-files-list")!.getBoundingClientRect();
  const position = { x: (inside ? rect.left + 30 : 10) * devicePixelRatio, y: (inside ? rect.top + 90 : 10) * devicePixelRatio };
  await emit("tauri://drag-enter", { paths, position });
  if (inside) assert(document.querySelector(".device-files-list.drop-active"), "valid drop highlights list");
  await emit("tauri://drag-drop", { paths, position });
}
function submitUploadNames(names: string[]): void {
  names.forEach((name, index) => {
    const input = document.querySelector<HTMLInputElement>(`[data-upload-name="${index}"]`)!;
    input.value = name; input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
}
function dragFile(path: string, distance = 15, surface = ".device-file-name"): void {
  const row = document.querySelector<HTMLElement>(`[data-file-path="${path}"]`)!;
  const target = row.querySelector<HTMLElement>(surface)!;
  const rect = target.getBoundingClientRect();
  const clientX = rect.left + Math.min(10, rect.width / 2), clientY = rect.top + rect.height / 2;
  target.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, buttons: 1, clientX, clientY }));
  document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, buttons: 1, clientX: clientX + distance, clientY }));
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, buttons: 0 }));
}
function selectFile(path: string, keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}): void {
  const row = document.querySelector<HTMLElement>(`[data-file-path="${path}"]`)!;
  row.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, buttons: 1, ...keys }));
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, buttons: 0, ...keys }));
  row.dispatchEvent(new MouseEvent("click", { bubbles: true, ...keys }));
}
document.querySelector("#preview-drop")!.addEventListener("click", async () => {
  if (!document.querySelector(".device-files")) {
    click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard"]'), "preview root listing ready");
  }
  await navigate("/sdcard");
  await dropFiles(["C:\\mock\\drag-日志.txt", "C:\\mock\\drag-config.json"]);
  await waitFor(() => document.querySelectorAll("[data-upload-name]").length === 2, "preview modal ready");
});
document.querySelector("#run-drag-tests")!.addEventListener("click", async () => {
  const run = document.querySelector<HTMLButtonElement>("#run-drag-tests")!;
  const result = document.querySelector("#ui-test-result")!;
  run.disabled = true;
  let passed = 0;
  const sources = ["C:\\mock\\drag-one.txt", "C:\\mock\\drag-two.txt"];
  const uploads = () => transferCalls().filter(call => call.args.request.direction === "upload");
  try {
    if (!document.querySelector(".device-files")) click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard"]'), "drag test root");
    await navigate("/sdcard");
    const before = uploads().length;
    await dropFiles(sources, false);
    assert(!document.querySelector("#device-files-dialog") && uploads().length === before, "outside drop does nothing"); ++passed;
    document.querySelector<HTMLInputElement>("#device-files-path")!.focus();
    await dropFiles(sources);
    await waitFor(() => document.querySelectorAll("[data-upload-name]").length === 2, "one rename field per file");
    assert(uploads().length === before, "drop doesn't upload before confirmation");
    assert(document.activeElement?.hasAttribute("data-upload-name"), "rename modal keeps focus instead of restoring background path input");
    assert(document.querySelector<HTMLInputElement>('[data-upload-name="0"]')!.value === "drag-one.txt", "default name keeps extension");
    click("#device-files-dialog-cancel");
    assert(uploads().length === before && !document.querySelector("#device-files-dialog"), "cancel entire rename dialog uploads nothing"); ++passed;
    await dropFiles([sources[0]]);
    await waitFor(() => document.querySelectorAll("[data-upload-name]").length === 1, "single file has one rename field");
    document.querySelector("#device-files-dialog")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    assert(!document.querySelector("#device-files-dialog") && uploads().length === before, "Escape cancels single upload without writes"); ++passed;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "rename dialog reopened");
    submitUploadNames(["same.txt", "same.txt"]);
    assert(document.querySelector(".device-files-dialog .error-text") && uploads().length === before, "duplicate names rejected");
    submitUploadNames(["../invalid", "valid.txt"]);
    assert(document.querySelector(".device-files-dialog .error-text") && uploads().length === before, "invalid names rejected");
    submitUploadNames(["drag-renamed-one.txt", "drag-renamed-two.txt"]);
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("上传完成：2/2") === true, "batch upload completed");
    assert(uploads().length === before + 2 && uploads().slice(-2).every(call => call.args.request.device_id === "TEST-A" && !call.args.request.overwrite), "each renamed item uploaded to bound device");
    assert(uploads().slice(-2).map(call => call.args.request.remote_path).join(",") === "/sdcard/drag-renamed-one.txt,/sdcard/drag-renamed-two.txt", "exact new names and current directory used"); ++passed;
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("upload-file"))!.disabled, "batch refresh finished");
    const beforeBound = uploads().length;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "bound queue setup");
    submitUploadNames(["drag-bound-one.txt", "drag-bound-two.txt"]);
    await waitFor(() => uploads().length === beforeBound + 1, "bound first item starts");
    await navigate("/system");
    click('[data-picker-id="adbDevice"]'); click('[data-picker-value="TEST-B"]');
    await waitFor(() => uploads().length === beforeBound + 2 && document.querySelector("[data-files-transfer-message]")?.textContent?.includes("上传完成：2/2") === true, "bound queue complete after selection/navigation changes");
    assert(uploads().slice(-2).every(call => call.args.request.device_id === "TEST-A" && call.args.request.remote_path.startsWith("/sdcard/drag-bound-")), "batch retains drop device and destination");
    click('[data-picker-id="adbDevice"]'); click('[data-picker-value="TEST-A"]');
    await navigate("/sdcard"); ++passed;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "conflict setup");
    const beforeConflict = uploads().length;
    submitUploadNames(["upload.txt", "drag-new.txt"]);
    await waitFor(() => document.querySelector("#device-files-dialog-title")?.textContent === "覆盖设备文件？", "fresh conflicts prompt");
    click("#device-files-dialog-cancel");
    assert(uploads().length === beforeConflict, "cancel overwrite cancels whole batch"); ++passed;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "overwrite setup");
    submitUploadNames(["upload.txt", "drag-new.txt"]);
    await waitFor(() => document.querySelector("#device-files-dialog-title")?.textContent === "覆盖设备文件？", "overwrite confirmed setup");
    const beforeOverwrite = uploads().length;
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => uploads().length === beforeOverwrite + 2 && document.querySelector("[data-files-transfer-message]")?.textContent?.includes("上传完成：2/2") === true, "overwrite batch complete");
    assert(uploads().slice(-2)[0].args.request.overwrite && !uploads().slice(-1)[0].args.request.overwrite, "only confirmed existing file overwritten"); ++passed;
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("upload-file"))!.disabled, "overwrite refresh finished");
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "directory conflict setup");
    submitUploadNames(["Download", "drag-unused.txt"]);
    await waitFor(() => document.querySelector(".device-files-dialog .error-text")?.textContent?.includes("同名目录") === true, "directory overwrite rejected");
    click("#device-files-dialog-cancel"); ++passed;
    const beforeCancel = uploads().length;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "cancel active queue setup");
    submitUploadNames(["drag-cancel-one.txt", "drag-cancel-two.txt"]);
    await waitFor(() => uploads().length === beforeCancel + 1 && !!document.querySelector(actions("cancel-transfer")), "first batch item started");
    click(actions("cancel-transfer"));
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已取消后续上传") === true, "whole pending queue cancelled");
    assert(uploads().length === beforeCancel + 1, "remaining files don't upload after cancellation"); ++passed;
    failNextUpload = true;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "failure queue setup");
    submitUploadNames(["drag-fail-one.txt", "drag-fail-two.txt"]);
    const beforeFail = uploads().length;
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("上传中止：0/2") === true, "failed batch stops");
    assert(uploads().length === beforeFail + 1, "upload failure stops remaining files"); ++passed;
    const dragCalls = () => calls.filter(call => call.command === "drag_device_file");
    const dragBefore = dragCalls().length;
    dragFile("/sdcard/upload.txt", 2);
    assert(dragCalls().length === dragBefore, "small mouse movement is still selection");
    dragFile("/sdcard/Download");
    assert(dragCalls().length === dragBefore, "directories use download button, not file drag");
    dragFile("/sdcard/upload.txt");
    await waitFor(() => dragCalls().length === dragBefore + 1, "native drag starts without save dialog");
    assert(dragCalls().at(-1)!.args.deviceId === "TEST-A" && dragCalls().at(-1)!.args.remotePaths.join(",") === "/sdcard/upload.txt", "drag binds correct device/file");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("拖到电脑文件夹") === true, "waiting drop state rendered");
    assert(!document.querySelector(".device-files-transfer progress"), "waiting for drop doesn't animate download progress");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已投放") === true, "mouse release accepted state is distinct from drag waiting");
    assert(!document.querySelector(".device-files-transfer progress"), "drop acceptance alone isn't download activity");
    await waitFor(() => !!document.querySelector(".device-files-transfer progress"), "actual content transfer enables progress");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已复制到目标文件夹") === true, "native drag result shown"); ++passed;
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("upload-file"))!.disabled, "drag refresh finished");
    selectFile("/sdcard/.hidden"); selectFile("/sdcard/upload.txt", { ctrlKey: true });
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 2, "Ctrl pointer selection keeps both files");
    assert(document.querySelector<HTMLButtonElement>(actions("rename"))!.disabled && !document.querySelector<HTMLButtonElement>(actions("delete"))!.disabled, "group delete enabled while rename remains single-item");
    for (let repeat = 0; repeat < 2; ++repeat) {
      rightClick('[data-file-path="/sdcard/.hidden"]');
      assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 2, "repeated right click preserves selected group");
      assert(!document.querySelector<HTMLButtonElement>(menuActions("delete"))!.disabled, "group context can delete the selected group");
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    }
    const beforeMulti = dragCalls().length;
    dragFile("/sdcard/.hidden");
    await waitFor(() => dragCalls().length === beforeMulti + 1, "multi-file native drag starts");
    assert(dragCalls().at(-1)!.args.remotePaths.join(",") === "/sdcard/.hidden,/sdcard/upload.txt", "pressing a selected row to drag doesn't collapse multi-selection");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已复制到目标文件夹（2 个文件）") === true, "multi-file copy complete");
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 2, "native copy preserves file selection"); ++passed;
    selectFile("/sdcard/.hidden"); selectFile("/sdcard/upload.txt", { shiftKey: true });
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 4, "Shift selects contiguous files");
    document.querySelector(".device-files-list")!.dispatchEvent(new KeyboardEvent("keydown", { key: "a", ctrlKey: true, bubbles: true }));
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === deviceA.get("/sdcard")!.length, "Ctrl+A includes entries but excludes parent navigation");
    document.querySelector(".device-files-list")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    assert(document.querySelectorAll('.device-files-table tr[aria-selected="true"]').length === 0, "Escape clears selection"); ++passed;
    const beforeCancelDrag = dragCalls().length; dragFile("/sdcard/upload.txt");
    await waitFor(() => dragCalls().length === beforeCancelDrag + 1 && document.querySelector("[data-files-transfer-message]")?.textContent?.includes("拖到电脑文件夹") === true, "pending native drag ready to cancel");
    click(actions("cancel-transfer"));
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已取消传输") === true, "cancelled pending drag stops waiting");
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("upload-file"))!.disabled, "cancelled drag unblocks file actions"); ++passed;
    const beforeRejected = dragCalls().length;
    failDrag = true; dragFile("/sdcard/upload.txt");
    await waitFor(() => dragCalls().length === beforeRejected + 1 && document.querySelector("[data-files-transfer-message]")?.textContent?.includes("拖到电脑文件夹") === true, "rejected target begins a distinct drag");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已取消传输") === true, "native drag cancellation shown"); ++passed;
    await dropFiles(["C:\\mock\\folder"]);
    await waitFor(() => document.querySelector(".device-files-footer .error-text")?.textContent?.includes("普通文件") === true, "directory input rejected clearly"); ++passed;
    const beforeClose = uploads().length;
    await dropFiles(sources);
    await waitFor(() => !!document.querySelector("[data-upload-name]"), "close batch setup");
    submitUploadNames(["drag-close-one.txt", "drag-close-two.txt"]);
    await waitFor(() => uploads().length === beforeClose + 1 && !!document.querySelector(actions("cancel-transfer")), "close batch first item started");
    click('[data-files-close="device-files:TEST-A"]');
    assert(document.querySelector("#device-files-dialog-title")?.textContent === "关闭文件标签？", "active batch close requires cancellation confirmation");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => !document.querySelector('[data-files-tab="device-files:TEST-A"]'), "close cancels batch then removes tab");
    assert(uploads().length === beforeClose + 1, "closed batch never starts subsequent item"); ++passed;
    result.textContent = `PASS：${passed} 组拖放回归；没有操作真实文件`;
  } catch (error) { result.textContent = `FAIL（拖放已过 ${passed} 组）：${String(error)}`; console.error(error); }
  finally { run.disabled = false; }
});

document.querySelector("#run-manager-tests")!.addEventListener("click", async () => {
  const run = document.querySelector<HTMLButtonElement>("#run-manager-tests")!;
  const result = document.querySelector("#ui-test-result")!;
  run.disabled = true;
  let passed = 0;
  const selected = () => [...document.querySelectorAll<HTMLElement>('.device-files-table tr[aria-selected="true"]')].map(row => row.dataset.filePath!);
  const pointer = (target: EventTarget, type: string, x: number, y: number, keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 13, button: 0, buttons: type === "pointerup" ? 0 : 1, clientX: x, clientY: y, ...keys }));
  const rectangle = (keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}) => {
    const list = document.querySelector<HTMLElement>(".device-files-list")!;
    const rect = list.getBoundingClientRect();
    const row = document.querySelector('[data-file-path="/sdcard/.hidden"]')!.getBoundingClientRect();
    const last = document.querySelector('[data-file-path="/sdcard/upload.txt"]')!.getBoundingClientRect();
    pointer(list, "pointerdown", rect.right - 25, last.bottom + 10, keys);
    pointer(document, "pointermove", rect.left + 25, row.top + 5, keys);
    assert(!!document.querySelector(".device-files-marquee"), "selection rectangle appears while left button held");
    pointer(document, "pointerup", rect.left + 25, row.top + 5, keys);
    list.dispatchEvent(new MouseEvent("click", { bubbles: true, ...keys }));
    assert(!document.querySelector(".device-files-marquee"), "mouse release removes rectangle");
  };
  try {
    await waitFor(() => !document.querySelector<HTMLButtonElement>("#device-files-open")!.disabled, "manager devices ready");
    click("#device-files-open"); await waitFor(() => !!document.querySelector('[data-file-path="/sdcard"]'), "manager root ready");
    await navigate("/sdcard");
    const beforeDrags = calls.filter(call => call.command === "drag_device_file").length;
    rectangle();
    assert(selected().length === 4 && !selected().includes("/sdcard/Download"), "reverse rectangle selects intersecting files, not the parent or other rows");
    assert(calls.filter(call => call.command === "drag_device_file").length === beforeDrags, "marquee doesn't invoke native download"); ++passed;
    for (const surface of ["td:first-child", "td:nth-child(2)", "td:nth-child(3)", "td:nth-child(4)", ".device-file-name"]) {
      const before = calls.filter(call => call.command === "drag_device_file").length;
      dragFile("/sdcard/.hidden", 15, surface);
      await waitFor(() => calls.filter(call => call.command === "drag_device_file").length === before + 1, `selected ${surface} starts download instead of marquee`);
      const request = calls.filter(call => call.command === "drag_device_file").at(-1)!.args;
      assert(request.remotePaths.length === 4 && request.remotePaths.includes("/sdcard/upload.txt"), "dragging any selected cell preserves entire marquee selection");
      assert(!document.querySelector(".device-files-marquee"), "selected row drag never starts a replacement marquee");
      await waitFor(() => !!document.querySelector(actions("cancel-transfer")) && document.querySelector("[data-files-transfer-message]")?.textContent?.includes("拖到电脑文件夹") === true, "new drag has entered its own waiting state");
      await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已复制到目标文件夹（4 个文件）") === true
        && !document.querySelector<HTMLButtonElement>(actions("upload-file"))!.disabled, "selected group drag completes");
      assert(selected().length === 4, "selected group is preserved after native drag");
    }
    ++passed;
    rectangle({ ctrlKey: true }); assert(selected().length === 0, "Ctrl rectangle toggles against initial selection");
    selectFile("/sdcard/Download"); rectangle({ shiftKey: true });
    assert(selected().length === 5 && selected().includes("/sdcard/Download"), "Shift rectangle adds to initial selection"); ++passed;
    const cell = document.querySelector<HTMLElement>('[data-file-path="/sdcard/upload.txt"] td:last-child')!;
    const cellRect = cell.getBoundingClientRect();
    pointer(cell, "pointerdown", cellRect.left + 10, cellRect.top + 10);
    pointer(document, "pointerup", cellRect.left + 10, cellRect.top + 10);
    cell.click(); assert(selected().join() === "/sdcard/upload.txt", "whitespace click without dragging still selects one item"); ++passed;
    selectFile("/sdcard/.hidden"); selectFile("/sdcard/upload.txt", { ctrlKey: true });
    const beforeDeletes = mutations().length;
    click(actions("delete"));
    assert(document.querySelector("#device-files-dialog-title")?.textContent === "删除 2 个项目？", "toolbar group delete asks once for the full group");
    assert(document.querySelector(".device-files-dialog-message")?.textContent?.includes("/sdcard/.hidden") && document.querySelector(".device-files-dialog-message")?.textContent?.includes("/sdcard/upload.txt"), "confirmation lists both bound paths");
    click("#device-files-dialog-cancel"); assert(mutations().length === beforeDeletes && selected().length === 2, "cancel group delete performs no mutations and keeps selection"); ++passed;
    // A deliberate mid-batch error must not continue deleting later selected files.
    deviceA.get("/sdcard")!.push(entry("/sdcard", "delete-a"), entry("/sdcard", "delete-b"), entry("/sdcard", "delete-c", true));
    click(actions("refresh")); await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/delete-c"]') && !document.querySelector<HTMLButtonElement>(actions("refresh"))!.disabled, "delete samples ready");
    selectFile("/sdcard/delete-a"); selectFile("/sdcard/delete-b", { ctrlKey: true }); selectFile("/sdcard/delete-c", { ctrlKey: true });
    failDeletePath = "/sdcard/delete-b";
    rightClick('[data-file-path="/sdcard/delete-a"]'); click(menuActions("delete"));
    assert(document.querySelector(".device-files-dialog-message")?.textContent?.includes("目录及其内容"), "mixed group warns about recursive directory deletion");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => document.querySelector(".device-files-footer .error-text")?.textContent?.includes("已删除 1/3") === true, "partial delete error is visible");
    assert(!document.querySelector('[data-file-path="/sdcard/delete-a"]') && selected().length === 2, "successful deletion refreshed while remaining items stay selected");
    assert(!mutations().slice(beforeDeletes).some(call => call.args.path === "/sdcard/delete-c"), "failure stops before later directory"); ++passed;
    document.querySelector(".device-files-list")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    assert(document.querySelector("#device-files-dialog-title")?.textContent === "删除 2 个项目？", "Delete key targets remaining group");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => document.querySelector(".device-files-footer")?.textContent?.includes("已删除 2 个项目") === true, "remaining mixed group deletion finished");
    assert(!document.querySelector('[data-file-path="/sdcard/delete-b"]') && !document.querySelector('[data-file-path="/sdcard/delete-c"]'), "both file and directory removed from refreshed listing"); ++passed;
    // Compute the marquee against full row indexes rather than just currently rendered DOM rows.
    await navigate("/large");
    const largeList = document.querySelector<HTMLElement>(".device-files-list")!;
    const largeRect = largeList.getBoundingClientRect();
    const firstCell = largeList.querySelector("tbody td")!;
    pointer(firstCell, "pointerdown", largeRect.right - 30, largeRect.top + 75);
    pointer(document, "pointermove", largeRect.left + 30, largeRect.bottom + 20);
    await delay(300);
    assert(largeList.scrollTop > 0, "marquee near lower edge auto-scrolls");
    largeList.scrollTop = 3200; largeList.dispatchEvent(new Event("scroll"));
    pointer(document, "pointermove", largeRect.left + 35, largeRect.top + 220);
    await delay(40);
    pointer(document, "pointerup", largeRect.left + 35, largeRect.top + 220);
    assert(document.querySelector("[data-files-selection-count]")?.textContent?.match(/已选 1\d\d 项/), "virtualized marquee includes non-rendered rows");
    const stoppedScroll = largeList.scrollTop; await delay(100); assert(largeList.scrollTop === stoppedScroll, "release stops auto-scrolling"); ++passed;
    await navigate("/sdcard");
    failDrag = true; dragFile("/sdcard/upload.txt");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已取消传输") === true, "mouse-release cancellation shown");
    assert(document.querySelector(".device-files-transfer progress")?.hasAttribute("value"), "cancelled progress has a fixed value, never indeterminate animation");
    const cancelledDrag = calls.filter(call => call.command === "drag_device_file").at(-1)!.args;
    await emit("device-file-transfer", { id: cancelledDrag.id, device_id: "TEST-A", phase: "running", message: "late stale tick", percent: null, bytes: 0, total: null, elapsed_ms: 999 });
    assert(!document.querySelector("[data-files-transfer-message]")?.textContent?.includes("late stale"), "late progress cannot revive cancelled transfer");
    await delay(3200); assert(!document.querySelector(".device-files-transfer"), "cancelled notice dismisses after three seconds"); ++passed;
    dragFile("/sdcard/upload.txt");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已复制到目标文件夹") === true, "successful transfer ready to dismiss");
    await delay(3200); assert(!document.querySelector(".device-files-transfer"), "successful notice dismisses after three seconds"); ++passed;
    await dropFiles(["C:\\mock\\drag-error.txt"]); await waitFor(() => !!document.querySelector("[data-upload-name]"), "failed upload setup");
    failNextUpload = true; submitUploadNames(["drag-error.txt"]);
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("上传中止") === true, "batch failure ready to dismiss");
    assert(document.querySelector(".device-files-transfer progress")?.hasAttribute("value"), "failed batch progress remains static");
    await delay(3200); assert(!document.querySelector(".device-files-transfer"), "failed batch notice dismisses after three seconds"); ++passed;
    // A previous notice's deadline cannot hide a newer active transfer, including in another tab.
    dragFile("/sdcard/upload.txt");
    await waitFor(() => document.querySelector("[data-files-transfer-message]")?.textContent?.includes("已复制到目标文件夹") === true, "old notice setup");
    await delay(2500);
    selectFile("/sdcard/.hidden"); click(actions("download")); await waitFor(() => !!document.querySelector("#device-files-dialog"), "new transfer overwrite prompt");
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => document.querySelector<HTMLProgressElement>(".device-files-transfer progress")?.value === 10, "new transfer running");
    click('[data-terminal-tab-id="serial"]'); await delay(700); click('[data-files-tab="device-files:TEST-A"]');
    assert(!!document.querySelector(".device-files-transfer"), "old notice timer doesn't hide newer running/just-completed transfer after switching tabs");
    await delay(3300); assert(!document.querySelector(".device-files-transfer"), "background completion also dismisses on its own deadline"); ++passed;
    result.textContent = `PASS：${passed} 组框选/删除/进度回归；仅模拟文件`;
  } catch (error) { result.textContent = `FAIL（框选已过 ${passed} 组）：${String(error)}`; console.error(error); }
  finally { run.disabled = false; }
});
