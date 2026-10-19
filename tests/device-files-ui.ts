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
let deviceAOnline = true;
let failNextSync = false;
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
  if (command === "transfer_device_file") {
    const record = { request: args.request, cancel: false };
    transfers.set(record.request.id, record);
    for (let percent = 10; percent <= 100; percent += 10) {
      await delay(80);
      if (record.cancel) break;
      await emit("device-file-transfer", { id: record.request.id, device_id: record.request.device_id, phase: "running", message: "模拟传输中", percent, bytes: percent * 1000, total: 100000, elapsed_ms: percent * 8 });
    }
    await emit("device-file-transfer", { id: record.request.id, device_id: record.request.device_id, phase: record.cancel ? "cancelled" : "completed", message: record.cancel ? "已取消传输" : "传输完成", percent: record.cancel ? null : 100, bytes: 100000, total: 100000, elapsed_ms: 800 });
    transfers.delete(record.request.id);
    if (record.cancel) throw new Error("已取消传输");
    return null;
  }
  if (command === "cancel_device_transfer") { const transfer = transfers.get(args.id); if (transfer) transfer.cancel = true; return null; }
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
