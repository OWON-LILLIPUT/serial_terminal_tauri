// Manual/automated browser fixture: npm run dev -- --port 1422,
// then open /tests/device-files-ui.html. All device/local filesystem commands are mocked.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { DeviceFileEntry } from "../src/device-files";

type EntryMap = Map<string, DeviceFileEntry[]>;
const calls: { command: string; args: any }[] = [];
const entry = (parent: string, name: string, dir = false): DeviceFileEntry => ({ name, path: `${parent === "/" ? "" : parent}/${name}`, is_dir: dir, kind: dir ? "directory" : "file", size: 24576, modified: 1700000000, permissions: dir ? "755" : "644" });
const deviceA: EntryMap = new Map([
  ["/", [entry("/", "sdcard", true), entry("/", "data", true), entry("/", "system", true)]],
  ["/sdcard", [entry("/sdcard", "Download", true), entry("/sdcard", ".hidden"), entry("/sdcard", "空格 ' $文件.txt"), entry("/sdcard", "<img onerror=alert(1)>.txt"), entry("/sdcard", "upload.txt")]],
  ["/sdcard/Download", []],
  ["/system", []],
]);
const deviceB: EntryMap = new Map([["/", [entry("/", "different-device.txt")]]]);
const files = new Map([["TEST-A", deviceA], ["TEST-B", deviceB]]);
const transfers = new Map<string, { request: any; cancel: boolean }>();
let deviceAOnline = true;
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
    await delay(args.path === "/slow" ? 150 : 10);
    const entries = files.get(args.deviceId)?.get(args.path);
    if (!entries) throw new Error("Permission denied: " + args.path);
    return { path: args.path, entries: structuredClone(entries) };
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
const mutations = () => calls.filter(call => call.command === "mutate_device_file");
const transferCalls = () => calls.filter(call => call.command === "transfer_device_file");
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
    click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard"]'), "root listing");
    assert(!document.querySelector("#rx-stat"), "file tab hides serial statistics");
    assert(document.querySelectorAll('[role="tab"][aria-selected="true"]').length === 1, "only file tab is selected"); ++passed;
    const row = document.querySelector<HTMLElement>('[data-file-path="/sdcard"]')!;
    row.click(); row.click(); row.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/Download"]'), "double click opens directory");
    assert(document.querySelector<HTMLInputElement>("#device-files-path")!.value === "/sdcard", "navigation updates input");
    assert(!document.querySelector(".device-files-table img"), "HTML filename is escaped"); ++passed;
    click(actions("mkdir")); const beforeCancel = mutations().length;
    click("#device-files-dialog-cancel"); assert(mutations().length === beforeCancel, "cancel mkdir has no writes");
    click(actions("mkdir")); submitName("测试目录");
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/测试目录"]'), "mkdir refreshes list"); ++passed;
    click('[data-file-path="/sdcard/.hidden"]'); click(actions("rename")); submitName("new ' $name.txt");
    await waitFor(() => mutations().some(call => call.args.name === "new ' $name.txt"), "rename exact name");
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "rename complete"); ++passed;
    click('[data-file-path="/sdcard/upload.txt"]'); click(actions("delete"));
    const beforeDelete = mutations().length; click("#device-files-dialog-cancel"); assert(mutations().length === beforeDelete, "cancel delete no writes");
    click('[data-file-path="/sdcard/测试目录"]'); click(actions("delete"));
    document.querySelector<HTMLFormElement>("#device-files-dialog")!.requestSubmit();
    await waitFor(() => mutations().length > beforeDelete, "delete confirmation");
    assert(mutations().at(-1)!.args.confirmed === true, "delete sends explicit confirmation");
    await waitFor(() => !document.querySelector<HTMLButtonElement>(actions("mkdir"))!.disabled, "delete complete"); ++passed;
    click(actions("upload-file"));
    await waitFor(() => !!document.querySelector("#device-files-dialog"), "upload overwrite dialog");
    const beforeTransfer = transferCalls().length; click("#device-files-dialog-cancel");
    assert(transferCalls().length === beforeTransfer, "cancel overwrite no transfer");
    click(actions("upload-file")); await waitFor(() => !!document.querySelector("#device-files-dialog"), "second upload dialog");
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
    click("#device-files-open");
    await waitFor(() => !!document.querySelector('[data-file-path="/different-device.txt"]'), "device B independent tab");
    assert(document.querySelectorAll("[data-files-tab]").length === 2, "two file tabs"); ++passed;
    click('[data-files-tab="device-files:TEST-A"]');
    await waitFor(() => !!document.querySelector('[data-file-path="/sdcard/upload.txt"]'), "return device A");
    click('[data-file-path="/sdcard/upload.txt"]'); click(actions("download"));
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
