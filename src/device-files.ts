import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";

export interface DeviceFileEntry {
  name: string;
  path: string;
  kind: "file" | "directory" | "symlink" | "other";
  is_dir: boolean;
  size: number;
  modified: number;
  permissions: string;
}

interface DirectoryListing { path: string; entries: DeviceFileEntry[] }
interface LocalSelection { path: string; name: string; is_dir: boolean; exists: boolean; size: number }
interface UploadItem { local: LocalSelection; name: string }
interface ClipboardStatus { id: string; device_id: string; message: string; error: boolean }
interface DirectoryHistoryEntry { path: string; scroll: number }
interface TransferProgress {
  id: string;
  device_id: string;
  phase: "dragging" | "running" | "completed" | "failed" | "cancelled";
  message: string;
  percent: number | null;
  bytes: number;
  total: number | null;
  elapsed_ms: number;
}
interface FileSession {
  id: string;
  deviceId: string;
  path: string;
  draftPath: string;
  history: DirectoryHistoryEntry[];
  historyIndex: number;
  entries: DeviceFileEntry[];
  directories: Map<string, DeviceFileEntry[]>;
  readAt: Map<string, number>;
  pendingReads: Map<string, { version: number; promise: Promise<DirectoryListing> }>;
  expanded: Set<string>;
  treeLoading: Map<string, number>;
  treeErrors: Map<string, string>;
  selected: string;
  selectedPaths: Set<string>;
  selectionAnchor: string;
  selectedNavigation: "" | "parent";
  loading: boolean;
  busy: boolean;
  request: number;
  error: string;
  message: string;
  scroll: number;
  treeScroll: number;
  cacheVersion: number;
  lastOnline: boolean;
  syncing: boolean;
  viewportHeight: number;
  transfer: TransferProgress | null;
  cancelling: boolean;
  batch: { cancelled: boolean } | null;
  dragging: boolean;
  clipboardId: string;
}
interface FileDialog {
  session: FileSession;
  title: string;
  message: string;
  input: string | null;
  accept: string;
  danger: boolean;
  error: string;
  submit: (value: string) => void;
  uploads?: UploadItem[];
}
interface FileHooks {
  activeId(): string;
  activate(id: string): void;
  render(): void;
  online(deviceId: string): boolean;
}
interface FileActionTarget { directory: string; entry?: DeviceFileEntry; entries?: DeviceFileEntry[]; multiple?: boolean }
interface MarqueeSelection {
  session: FileSession;
  list: HTMLElement;
  pointerId: number;
  startX: number;
  startY: number;
  clientX: number;
  clientY: number;
  rowTop: number;
  base: Set<string>;
  mode: "replace" | "add" | "toggle";
  active: boolean;
  overlay: HTMLElement | null;
  frame: number;
}
const fileMenuActions = [
  ["up", "上一级", "E74A"], ["go", "前往", "E8A7"], ["refresh", "刷新", "E72C"],
  ["upload-file", "上传文件", "E898"], ["upload-directory", "上传目录", "E8B7"],
  ["download", "下载", "E896"], ["mkdir", "新建目录", "E8F4"],
  ["copy", "复制（Ctrl+C）", "E8C8"], ["paste", "粘贴（Ctrl+V）", "E77F"],
  ["rename", "重命名", "E8AC"], ["delete", "删除", "E74D"], ["sync", "同步到硬盘", "E74E"],
] as const;

export function normalizeDevicePath(path: string): string {
  if (!path.startsWith("/") || /[\x00-\x1f\x7f]/.test(path)) throw new Error("请输入以 / 开头的设备绝对路径");
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "..") parts.pop();
    else if (part && part !== ".") parts.push(part);
  }
  return `/${parts.join("/")}`;
}
export function validDeviceName(name: string): boolean {
  return !!name && name !== "." && name !== ".." && !/[\/\x00-\x1f\x7f]/.test(name)
    && new TextEncoder().encode(name).length <= 255;
}
function parentPath(path: string): string { return path.slice(0, path.lastIndexOf("/")) || "/"; }
function escape(value: string): string {
  return value.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}
function message(error: unknown): string { return error instanceof Error ? error.message : String(error); }
function sizeText(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 ** 2) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 ** 3) return `${(size / 1024 ** 2).toFixed(1)} MB`;
  return `${(size / 1024 ** 3).toFixed(2)} GB`;
}
function transferRunning(session: FileSession): boolean { return session.dragging || !!session.batch || session.transfer?.phase === "running"; }
const directoryCacheMs = 30000;
const fileRowHeight = 32;
const transferNoticeMs = 3000;
const directoryHistoryLimit = 128;
const fileDateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "medium" });
function modifiedText(seconds: number): string {
  const date = new Date(seconds * 1000);
  return Number.isFinite(date.getTime()) ? fileDateFormat.format(date) : "—";
}

export class DeviceFiles {
  private sessions = new Map<string, FileSession>();
  private dialog: FileDialog | null = null;
  private ready: Promise<void>;
  private closing = new Set<string>();
  private pendingPanels = new Set<string>();
  private viewportObserver: ResizeObserver | null = null;
  private contextMenu: HTMLElement | null = null;
  private dragCandidate: { session: FileSession; entries: DeviceFileEntry[]; x: number; y: number } | null = null;
  private pointerSelection: { session: FileSession; path: string; collapse: boolean } | null = null;
  private marquee: MarqueeSelection | null = null;
  private suppressSelectionClick = "";
  private transferTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private dropTarget: { session: FileSession; directory: string } | null = null;
  private focus: { id: string; start: number | null; end: number | null } | null = null;

  constructor(private hooks: FileHooks) {
    this.ready = Promise.all([
      listen<TransferProgress>("device-file-transfer", event => this.onProgress(event.payload)),
      listen<ClipboardStatus>("device-file-clipboard", event => {
        const status = event.payload, session = this.sessions.get(`device-files:${status.device_id}`);
        if (!session || session.clipboardId !== status.id) return;
        if (status.error) session.error = status.message;
        else session.message = status.message;
        this.update(session);
      }),
      getCurrentWebview().onDragDropEvent(event => this.onDragDrop(event.payload)),
    ]).then(() => {});
    // Report listener errors on the file page rather than creating an unhandled rejection.
    void this.ready.catch(() => {});
    for (const type of ["mousedown", "mouseup", "auxclick"] as const) {
      document.addEventListener(type, event => this.onHistoryMouse(event), { capture: true });
    }
    document.addEventListener("pointerdown", event => {
      this.suppressSelectionClick = "";
      if (this.contextMenu && !(event.target instanceof Node && this.contextMenu.contains(event.target))) this.closeContextMenu();
    });
    document.addEventListener("pointermove", event => {
      if (this.marquee) { this.moveMarquee(event); return; }
      const candidate = this.dragCandidate;
      if (!candidate) return;
      if (!(event.buttons & 1)) { this.dragCandidate = null; return; }
      if (Math.hypot(event.clientX - candidate.x, event.clientY - candidate.y) < 6) return;
      this.dragCandidate = null;
      event.preventDefault();
      void this.dragDownload(candidate.session, candidate.entries);
    });
    document.addEventListener("pointerup", event => { if (this.marquee?.pointerId === event.pointerId) this.endMarquee(); this.dragCandidate = null; });
    document.addEventListener("pointercancel", () => { this.endMarquee(); this.dragCandidate = null; this.pointerSelection = null; });
    document.addEventListener("keydown", event => {
      const session = this.sessions.get(this.hooks.activeId());
      const target = event.target instanceof Element ? event.target : null;
      const key = event.key.toLowerCase();
      if (session && !event.defaultPrevented && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey
        && ["c", "v"].includes(key) && !this.dialog
        && target?.closest(".device-files, .device-files-context-menu")
        && !target.closest("input, textarea, [contenteditable]")) {
        event.preventDefault(); event.stopPropagation();
        if (!event.repeat) void this.action(session, key === "c" ? "copy" : "paste");
        return;
      }
      if (session && ["BrowserBack", "BrowserForward"].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        if (!event.repeat) void this.travelHistory(session, event.key === "BrowserBack" ? -1 : 1);
        return;
      }
      if (this.marquee && event.key === "Escape") { this.endMarquee(); event.preventDefault(); return; }
      if (!this.contextMenu || !["Escape", "Tab"].includes(event.key)) return;
      this.closeContextMenu();
      if (event.key === "Escape") { event.preventDefault(); document.querySelector<HTMLElement>(".device-files-list")?.focus(); }
    });
    document.addEventListener("scroll", event => {
      if (this.contextMenu && !(event.target instanceof Node && this.contextMenu.contains(event.target))) this.closeContextMenu();
    }, true);
    window.addEventListener("resize", () => { this.endMarquee(); this.closeContextMenu(); });
    window.addEventListener("blur", () => { this.endMarquee(); this.closeContextMenu(); this.dragCandidate = null; this.clearDropTarget(); });
  }

  has(id: string): boolean { return this.sessions.has(id); }
  open(deviceId: string): void {
    const id = `device-files:${deviceId}`;
    let session = this.sessions.get(id);
    if (!session) {
      session = {
        id, deviceId, path: "/", draftPath: "/", history: [], historyIndex: -1, entries: [], directories: new Map(), readAt: new Map(), pendingReads: new Map(),
        expanded: new Set(["/"]), treeLoading: new Map(), treeErrors: new Map(), selected: "", selectedNavigation: "", selectedPaths: new Set(), selectionAnchor: "",
        loading: false, busy: false, request: 0, error: "", message: "", scroll: 0, treeScroll: 0,
        transfer: null, cancelling: false, batch: null, dragging: false, clipboardId: "", cacheVersion: 0, lastOnline: this.hooks.online(deviceId), syncing: false, viewportHeight: 600,
      };
      this.sessions.set(id, session);
      this.hooks.activate(id);
      void this.navigate(session, "/");
    } else {
      this.hooks.activate(id);
      if (session.error) void this.navigate(session, session.path);
    }
  }

  renderTabs(): string {
    return [...this.sessions.values()].map(session => `
      <button class="terminal-tab ${session.id === this.hooks.activeId() ? "active" : ""}" type="button"
        role="tab" aria-selected="${session.id === this.hooks.activeId()}" data-files-tab="${escape(session.id)}"
        title="设备文件 · ${escape(session.deviceId)}">
        <span class="small-dot ${this.hooks.online(session.deviceId) ? "connected" : "disconnected"}"></span>
        <span class="terminal-tab-label">设备文件 · ${escape(session.deviceId)}</span>
        <span class="terminal-tab-close" data-files-close="${escape(session.id)}" title="关闭设备文件">×</span>
      </button>`).join("");
  }

  updateConnectivity(): void {
    let changed = false;
    for (const session of this.sessions.values()) {
      const online = this.hooks.online(session.deviceId);
      if (session.lastOnline !== online) { session.lastOnline = online; session.readAt.clear(); ++session.cacheVersion; changed = true; }
    }
    if (changed) this.hooks.render();
  }

  capture(): void {
    const session = this.sessions.get(this.hooks.activeId());
    if (!session) return;
    // A completed navigation resets scroll; don't capture the old directory's viewport.
    const root = document.querySelector<HTMLElement>(".device-files");
    if (root?.dataset.fileSession !== session.id) { this.focus = null; return; }
    if (root?.dataset.fileDirectory === session.path) session.scroll = root.querySelector(".device-files-list")?.scrollTop ?? session.scroll;
    session.treeScroll = document.querySelector(".device-files-tree")?.scrollTop ?? session.treeScroll;
    const focused = document.activeElement;
    this.focus = focused instanceof HTMLInputElement && focused.id.startsWith("device-files-")
      ? { id: focused.id, start: focused.selectionStart, end: focused.selectionEnd } : null;
  }

  render(id: string): string {
    const session = this.sessions.get(id);
    if (!session) return "";
    const online = this.hooks.online(session.deviceId);
    const disabled = (action: string) => this.actionDisabled(session, action) ? "disabled" : "";
    return `<div class="device-files" data-file-session="${escape(id)}" data-file-directory="${escape(session.path)}">
      <div class="device-files-toolbar">
        <button class="secondary-button" data-files-action="up" type="button" title="上一级" ${disabled("up")}>↑</button>
        <form id="device-files-path-form"><input id="device-files-path" class="text-input" aria-label="设备目录路径" value="${escape(session.draftPath)}" />
          <button class="secondary-button" type="submit" ${disabled("go")}>前往</button></form>
        <button class="secondary-button" data-files-action="refresh" type="button" ${disabled("refresh")}>刷新</button>
      </div>
      <div class="device-files-actions">
        <button class="secondary-button" data-files-action="upload-file" type="button" ${disabled("upload-file")}>上传文件</button>
        <button class="secondary-button" data-files-action="upload-directory" type="button" ${disabled("upload-directory")}>上传目录</button>
        <button class="secondary-button" data-files-action="download" type="button" ${disabled("download")}>下载</button>
        <button class="secondary-button" data-files-action="mkdir" type="button" ${disabled("mkdir")}>新建目录</button>
        <button class="secondary-button" data-files-action="rename" type="button" ${disabled("rename")}>重命名</button>
        <button class="secondary-button danger-button" data-files-action="delete" type="button" ${disabled("delete")}>删除</button>
        <button class="secondary-button" data-files-action="sync" type="button" title="执行设备端 adb shell sync，将缓存写入设备存储，不是备份到电脑" ${disabled("sync")}>${session.syncing ? "正在同步…" : "同步到硬盘"}</button>
        <span class="device-files-device" title="${escape(session.deviceId)}">${escape(session.deviceId)}</span>
      </div>
      ${!online ? '<div class="device-files-notice">设备未连接或未授权，请重新连接后刷新。此标签仍绑定原设备。</div>' : ""}
      <div class="device-files-body">
        <nav class="device-files-tree" aria-label="设备目录树">${this.renderTree(session, "/", 0, new Set())}</nav>
        <div class="device-files-list" tabindex="0" aria-label="设备文件列表" title="空白处按住左键框选；Ctrl+C 复制设备文件或文件夹到电脑粘贴下载；Ctrl+V 粘贴电脑文件或文件夹上传；支持拖入/拖出普通文件">
          <table class="device-files-table"><colgroup><col class="device-files-name-column"><col class="device-files-size-column"><col class="device-files-modified-column"><col class="device-files-permissions-column"></colgroup>
            <thead><tr><th>名称</th><th>大小</th><th>修改时间</th><th>权限</th></tr></thead>
            <tbody>${this.renderRows(session)}</tbody></table>
          ${session.loading ? '<div class="device-files-empty">正在读取目录…</div>' : !session.entries.length && !session.error ? '<div class="device-files-empty">此目录为空</div>' : ""}
          <div class="device-files-drop-hint" aria-hidden="true">松开上传到当前目录<br><span>先逐个确认文件名称，再开始上传</span></div>
        </div>
      </div>
      <div class="device-files-footer"><span class="${session.error ? "error-text" : ""}" title="${escape(session.error || session.message || session.path)}">${escape(session.error || session.message || `${session.entries.length} 个项目 · ${session.path}`)}</span><span data-files-selection-count>${session.selectedPaths.size > 1 ? `已选 ${session.selectedPaths.size} 项` : ""}</span></div>
      ${this.renderTransfer(session)}
    </div>`;
  }

  private rowRange(session: FileSession): [number, number] {
    const start = Math.max(0, Math.floor((session.scroll - fileRowHeight) / fileRowHeight) - 8);
    const count = session.entries.length + 1;
    return [Math.min(start, count), Math.min(count, start + Math.ceil(session.viewportHeight / fileRowHeight) + 20)];
  }

  private renderRows(session: FileSession): string {
    const [start, end] = this.rowRange(session);
    const spacer = (height: number) => height > 0 ? `<tr class="device-files-spacer" aria-hidden="true"><td colspan="4" style="height:${height}px"></td></tr>` : "";
    // Navigation rows aren't device entries: never include them in mutations or
    // the directory tree, and only construct the visible slice for large folders.
    return spacer(start * fileRowHeight) + Array.from({ length: end - start }, (_, offset) => {
      const index = start + offset;
      const navigation = index === 0 ? "parent" : "";
      const entry: DeviceFileEntry = navigation ? { name: "..", path: parentPath(session.path),
        kind: "directory", is_dir: true, size: 0, modified: 0, permissions: "—" } : session.entries[index - 1];
      const selected = navigation ? entry.path === session.selected && navigation === session.selectedNavigation : !session.selectedNavigation && session.selectedPaths.has(entry.path);
      const modified = navigation ? "—" : modifiedText(entry.modified);
      return `<tr data-file-path="${escape(entry.path)}" data-file-navigation="${navigation}" class="${selected ? "selected" : ""}" aria-selected="${selected}">
        <td title="${escape(entry.path)}"><span class="device-file-name"><span class="device-file-icon ${entry.is_dir ? "folder" : ""}">${entry.is_dir ? "▸" : entry.kind === "symlink" ? "↗" : "▤"}</span>${escape(entry.name)}${navigation ? '<span class="device-file-link">上级目录</span>' : entry.kind === "symlink" ? '<span class="device-file-link">链接</span>' : ""}</span></td>
        <td>${entry.is_dir ? "—" : sizeText(entry.size)}</td><td title="${escape(modified)}">${escape(modified)}</td><td title="${escape(entry.permissions)}">${escape(entry.permissions)}</td>
      </tr>`;
    }).join("") + spacer((session.entries.length + 1 - end) * fileRowHeight);
  }

  private renderTree(session: FileSession, path: string, depth: number, seen: Set<string>): string {
    if (depth > 20 || seen.has(path)) return "";
    const name = path === "/" ? "/ 根目录" : path.slice(path.lastIndexOf("/") + 1);
    const expanded = session.expanded.has(path);
    const loading = session.treeLoading.get(path) === session.cacheVersion;
    const children = session.directories.get(path)?.filter(entry => entry.is_dir) ?? [];
    const nextSeen = new Set(seen).add(path);
    return `<div class="device-files-tree-node">
      <div class="device-files-tree-row ${path === session.path ? "selected" : ""}" style="--tree-depth:${depth}">
        <button class="device-files-tree-toggle" type="button" data-files-expand="${escape(path)}" aria-label="${expanded ? "折叠" : "展开"} ${escape(name)}" aria-expanded="${expanded}" ${loading ? "disabled" : ""}>${loading ? "·" : expanded ? "▾" : "▸"}</button>
        <button class="device-files-tree-name" type="button" data-files-directory="${escape(path)}" title="${escape(path)}">${escape(name)}</button>
      </div>
      ${expanded ? (session.treeErrors.has(path) ? `<div class="device-files-tree-error" title="${escape(session.treeErrors.get(path)!)}">无法读取，请重试</div>` : children.map(entry => this.renderTree(session, entry.path, depth + 1, nextSeen)).join("")) : ""}
    </div>`;
  }

  private renderTransfer(session: FileSession): string {
    const transfer = session.transfer;
    if (!transfer) return "";
    const active = transfer.phase === "running" || transfer.phase === "dragging";
    const percent = transfer.phase === "completed" ? 100 : transfer.percent == null ? (active && !session.cancelling ? null : 0) : Math.max(0, Math.min(100, transfer.percent));
    const detail = [transfer.percent != null ? `${transfer.percent}%` : "", transfer.bytes ? sizeText(transfer.bytes) : ""].filter(Boolean).join(" · ");
    return `<div class="device-files-transfer" aria-live="polite">
      <div class="device-files-transfer-text"><span data-files-transfer-message class="${transfer.phase === "failed" ? "error-text" : ""}" title="${escape(transfer.message)}">${escape(transfer.message)}</span><span data-files-transfer-detail>${escape(detail)}</span></div>
      ${transfer.phase !== "dragging" ? `<progress ${percent != null ? `value="${percent}" max="100"` : ""} aria-label="文件传输进度"></progress>` : ""}
      ${transfer.phase === "running" || transfer.phase === "dragging" ? `<button class="secondary-button" data-files-action="cancel-transfer" type="button" ${session.cancelling ? "disabled" : ""}>${session.cancelling ? "正在取消…" : transfer.phase === "dragging" ? "取消拖动" : "取消传输"}</button>` : '<button class="secondary-button" data-files-action="dismiss-transfer" type="button">收起</button>'}
    </div>`;
  }

  renderDialog(): string {
    const dialog = this.dialog;
    if (!dialog) return "";
    return `<div class="device-files-dialog-backdrop"><form id="device-files-dialog" class="device-files-dialog" role="dialog" aria-modal="true" aria-labelledby="device-files-dialog-title">
      <div id="device-files-dialog-title" class="device-files-dialog-title">${escape(dialog.title)}</div>
      <div class="device-files-dialog-message">${escape(dialog.message)}</div>
      ${dialog.input != null ? `<input id="device-files-dialog-input" class="text-input" aria-label="名称" value="${escape(dialog.input)}" maxlength="255" autocomplete="off" />` : ""}
      ${dialog.uploads ? `<div class="device-files-upload-names">${dialog.uploads.map((item, index) => `<label class="device-files-upload-name">
        <span title="${escape(item.local.path)}">${escape(item.local.name)} · ${item.local.is_dir ? "文件夹（包含子目录）" : sizeText(item.local.size)}</span>
        <input id="device-files-upload-name-${index}" class="text-input" data-upload-name="${index}" aria-label="文件 ${index + 1} 上传名称" value="${escape(item.name)}" maxlength="255" autocomplete="off" />
      </label>`).join("")}</div>` : ""}
      ${dialog.error ? `<div class="error-text">${escape(dialog.error)}</div>` : ""}
      <div class="device-files-dialog-actions"><button class="secondary-button" id="device-files-dialog-cancel" type="button">取消</button>
        <button class="secondary-button ${dialog.danger ? "danger-button" : ""}" type="submit">${escape(dialog.accept)}</button></div>
    </form></div>`;
  }

  bind(): void {
    this.endMarquee();
    this.closeContextMenu();
    this.dragCandidate = null;
    this.pointerSelection = null;
    this.clearDropTarget();
    document.querySelectorAll<HTMLElement>("[data-files-tab]").forEach(button => button.addEventListener("click", event => {
      const close = (event.target as Element)?.closest<HTMLElement>("[data-files-close]");
      if (close?.dataset.filesClose) { void this.close(close.dataset.filesClose); return; }
      if (button.dataset.filesTab) this.hooks.activate(button.dataset.filesTab);
    }));
    this.bindPanel();
    this.bindDialog();
    this.restoreFocus();
  }

  private bindPanel(): void {
    this.endMarquee();
    this.viewportObserver?.disconnect();
    this.viewportObserver = null;
    const session = this.sessions.get(this.hooks.activeId());
    if (session) {
      const list = document.querySelector<HTMLElement>(".device-files-list");
      const tree = document.querySelector<HTMLElement>(".device-files-tree");
      if (list) list.scrollTop = session.scroll;
      if (tree) tree.scrollTop = session.treeScroll;
      if (list) session.viewportHeight = list.clientHeight;
      // The first render may use the previous tab's viewport height. The observer
      // must reconcile it even when scrollTop itself hasn't changed.
      let range = "";
      let scrollQueued = false;
      const updateViewport = () => {
        if (!list) return;
        session.scroll = list.scrollTop;
        session.viewportHeight = list.clientHeight;
        if (scrollQueued) return;
        scrollQueued = true;
        requestAnimationFrame(() => {
          scrollQueued = false;
          if (!list.isConnected || !this.live(session)) return;
          const next = this.rowRange(session).join(":");
          if (next === range) return;
          range = next;
          const body = list.querySelector("tbody");
          if (body) body.innerHTML = this.renderRows(session);
        });
      };
      list?.addEventListener("scroll", updateViewport, { passive: true });
      if (list) { this.viewportObserver = new ResizeObserver(updateViewport); this.viewportObserver.observe(list); }
      document.querySelector("#device-files-path-form")?.addEventListener("submit", event => {
        event.preventDefault();
        void this.action(session, "go");
      });
      document.querySelector<HTMLInputElement>("#device-files-path")?.addEventListener("input", event => { session.draftPath = (event.target as HTMLInputElement).value; });
      document.querySelectorAll<HTMLElement>("[data-files-action]").forEach(button => button.addEventListener("click", () => { void this.action(session, button.dataset.filesAction!); }));
      list?.addEventListener("click", event => {
          if (session.dragging) return;
          if (this.suppressSelectionClick === session.id) { this.suppressSelectionClick = ""; return; }
          const row = (event.target as Element)?.closest<HTMLElement>("[data-file-path]");
          if (!row) { if (!event.ctrlKey && !event.metaKey && !event.shiftKey) this.select(session, ""); return; }
          const handled = this.pointerSelection;
          this.pointerSelection = null;
          if (handled?.session !== session || handled.path !== row.dataset.filePath || handled.collapse) {
            this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"], event);
          }
          list?.focus();
        });
      list?.addEventListener("pointerdown", event => {
        if (event.button !== 0 || this.dialog) return;
        const target = event.target as Element;
        if (target.closest("thead") || !list || session.loading || session.busy) return;
        const row = target.closest<HTMLElement>("[data-file-path]");
        // Once selected, the entire row is a drag handle, including details/whitespace.
        // Otherwise whitespace can begin a new marquee; Ctrl/Shift preserve that option.
        const dragSelected = row && !row.dataset.fileNavigation && session.selectedPaths.has(row.dataset.filePath!)
          && !event.ctrlKey && !event.metaKey && !event.shiftKey;
        if (!row || (!dragSelected && target.closest("td") && !target.closest(".device-file-name"))) {
          this.beginMarquee(session, list, event); return;
        }
        const path = row.dataset.filePath!;
        const collapse = !event.ctrlKey && !event.metaKey && !event.shiftKey && session.selectedPaths.size > 1 && session.selectedPaths.has(path);
        if (!collapse) this.select(session, path, row.dataset.fileNavigation as FileSession["selectedNavigation"], event);
        this.pointerSelection = { session, path, collapse };
        if (row.dataset.fileNavigation) return;
        const entry = session.entries.find(entry => entry.path === row.dataset.filePath);
        if (!entry || entry.kind !== "file" || entry.is_dir || this.actionDisabled(session, "download", { directory: session.path, entry })) return;
        if (!session.selectedPaths.has(path)) return;
        this.dragCandidate = { session, entries: session.entries.filter(entry => session.selectedPaths.has(entry.path)), x: event.clientX, y: event.clientY };
      });
      list?.addEventListener("dblclick", event => {
          const row = (event.target as Element)?.closest<HTMLElement>("[data-file-path]");
          if (!row) return;
          this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"]);
          this.openSelectedDirectory(session);
        });
      list?.addEventListener("keydown", event => {
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
          event.preventDefault(); session.selectedPaths = new Set(session.entries.map(entry => entry.path));
          session.selected = session.entries[session.entries.length - 1]?.path ?? ""; session.selectedNavigation = "";
          session.selectionAnchor = session.entries[0]?.path ?? ""; this.paintSelection(session);
        } else if (event.key === "Escape") {
          event.preventDefault(); this.select(session, "");
        } else if (event.key === "Enter") {
          event.preventDefault(); this.openSelectedDirectory(session);
        } else if (event.key === "Delete") {
          event.preventDefault(); void this.action(session, "delete");
        }
      });
      document.querySelectorAll<HTMLElement>("[data-files-directory]").forEach(button => button.addEventListener("click", () => { void this.openTreeDirectory(session, button.dataset.filesDirectory!); }));
      document.querySelectorAll<HTMLElement>("[data-files-expand]").forEach(button => button.addEventListener("click", () => { void this.expand(session, button.dataset.filesExpand!); }));
      document.querySelector(".device-files")?.addEventListener("contextmenu", event => {
        const mouse = event as MouseEvent;
        const target = mouse.target instanceof Element ? mouse.target : null;
        if (!target?.closest(".device-files-list, .device-files-tree") || target.closest("input, textarea, [contenteditable]") || this.dialog) return;
        mouse.preventDefault(); mouse.stopPropagation();
        const row = target.closest<HTMLElement>("[data-file-path]");
        const treeRow = target.closest<HTMLElement>(".device-files-tree-row");
        if (row) {
          if (!session.selectedPaths.has(row.dataset.filePath!) || row.dataset.fileNavigation) this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"]);
        }
        else if (target.closest(".device-files-list")) this.select(session, "");
        const directory = treeRow?.querySelector<HTMLElement>("[data-files-directory]")?.dataset.filesDirectory;
        const context = directory ? {
          directory, entry: session.directories.get(parentPath(directory))?.find(entry => entry.path === directory),
        } : this.actionTarget(session);
        this.openContextMenu(session, context, mouse.clientX, mouse.clientY);
        treeRow?.classList.add("context-target");
      });
    }
  }

  private bindDialog(): void {
    const form = document.querySelector<HTMLFormElement>("#device-files-dialog");
    if (form) {
      document.querySelector("#device-files-dialog-cancel")?.addEventListener("click", () => { this.dialog = null; this.hooks.render(); });
      form.addEventListener("keydown", event => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); this.dialog = null; this.hooks.render(); } });
      form.addEventListener("submit", event => {
        event.preventDefault();
        const dialog = this.dialog;
        if (!dialog) return;
        if (dialog.uploads) {
          const names = dialog.uploads.map(item => item.name);
          if (names.some(name => !validDeviceName(name)) || new Set(names).size !== names.length) {
            dialog.error = "每个名称必须有效且不能重复（不能含 /、控制字符或为 . / ..，最多 255 字节）";
            this.hooks.render(); return;
          }
        }
        const value = document.querySelector<HTMLInputElement>("#device-files-dialog-input")?.value ?? "";
        if (dialog.input != null && !validDeviceName(value)) { dialog.error = "名称不能为空，不能包含 /、控制字符，也不能是 . 或 ..（最多 255 字节）"; this.hooks.render(); return; }
        this.dialog = null;
        // Local panel redraws must not leave an already accepted modal behind.
        this.hooks.render();
        dialog.submit(value);
      });
      const input = document.querySelector<HTMLInputElement>("#device-files-dialog-input");
      form.querySelectorAll<HTMLInputElement>("[data-upload-name]").forEach(field => field.addEventListener("input", () => {
        const item = this.dialog?.uploads?.[Number(field.dataset.uploadName)];
        if (item) item.name = field.value;
      }));
      input?.addEventListener("input", () => { if (this.dialog) this.dialog.input = input.value; });
      (input ?? form.querySelector<HTMLInputElement>("[data-upload-name]") ?? form.querySelector<HTMLButtonElement>("#device-files-dialog-cancel"))?.focus();
      if (this.focus?.id !== "device-files-dialog-input") input?.select();
      // Keep keyboard focus inside the confirmation rather than reaching device controls behind it.
      form.addEventListener("keydown", event => {
        if (event.key !== "Tab") return;
        const controls = [...form.querySelectorAll<HTMLElement>("input, button")];
        const first = controls[0], last = controls[controls.length - 1];
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      });
    }
  }

  private restoreFocus(): void {
    if (this.focus) {
      const input = document.getElementById(this.focus.id);
      if (input instanceof HTMLInputElement && (!this.dialog || input.closest("#device-files-dialog"))) {
        input.focus();
        if (this.focus.start != null && this.focus.end != null) input.setSelectionRange(this.focus.start, this.focus.end);
      }
      this.focus = null;
    }
  }

  private update(session: FileSession): void {
    if (session.id !== this.hooks.activeId() || this.pendingPanels.has(session.id)) return;
    this.pendingPanels.add(session.id);
    requestAnimationFrame(() => {
      this.pendingPanels.delete(session.id);
      if (!this.live(session) || session.id !== this.hooks.activeId()) return;
      const root = document.querySelector<HTMLElement>(".device-files");
      if (!root?.parentElement || root.dataset.fileSession !== session.id) return;
      this.closeContextMenu();
      this.capture();
      const panel = root.parentElement;
      panel.innerHTML = this.render(session.id);
      this.bindPanel();
      this.restoreFocus();
    });
  }
  private live(session: FileSession): boolean { return this.sessions.get(session.id) === session; }

  private readDirectory(session: FileSession, path: string, force = false): Promise<DirectoryListing> {
    const pending = session.pendingReads.get(path);
    if (pending?.version === session.cacheVersion) return pending.promise;
    const cached = session.directories.get(path);
    if (!force && cached && Date.now() - (session.readAt.get(path) ?? 0) < directoryCacheMs) {
      return Promise.resolve({ path, entries: cached });
    }
    const version = session.cacheVersion;
    const promise = invoke<DirectoryListing>("list_device_directory", { deviceId: session.deviceId, path }).then(result => {
      if (this.live(session) && version === session.cacheVersion) {
        session.directories.set(path, result.entries);
        session.readAt.set(path, Date.now());
        session.treeErrors.delete(path);
      }
      return result;
    }).finally(() => { if (session.pendingReads.get(path)?.promise === promise) session.pendingReads.delete(path); });
    session.pendingReads.set(path, { version, promise });
    return promise;
  }

  private onHistoryMouse(event: MouseEvent): void {
    if (event.button !== 3 && event.button !== 4) return;
    const session = this.sessions.get(this.hooks.activeId());
    if (!session) return;
    // Suppress the WebView's own page history on every phase, but navigate only once.
    event.preventDefault(); event.stopPropagation();
    if (event.type === "mouseup") void this.travelHistory(session, event.button === 3 ? -1 : 1);
  }

  private async travelHistory(session: FileSession, direction: -1 | 1): Promise<void> {
    if (this.dialog || session.id !== this.hooks.activeId() || session.dragging || this.actionDisabled(session, "go")) return;
    const index = session.historyIndex + direction;
    const target = session.history[index];
    if (!target) return;
    this.endMarquee(); this.dragCandidate = null; this.pointerSelection = null;
    this.closeContextMenu();
    await this.navigate(session, target.path, false, index);
  }

  private saveHistoryScroll(session: FileSession): void {
    const current = session.history[session.historyIndex];
    if (current?.path !== session.path) return;
    const root = document.querySelector<HTMLElement>(".device-files");
    if (session.id === this.hooks.activeId() && root?.dataset.fileSession === session.id && root.dataset.fileDirectory === session.path) {
      session.scroll = root.querySelector(".device-files-list")?.scrollTop ?? session.scroll;
    }
    current.scroll = session.scroll;
  }

  private commitDirectoryHistory(session: FileSession, path: string, index?: number): number {
    if (index != null) {
      if (session.history[index]?.path !== path) throw new Error("目录历史已失效，请重新打开目录");
      session.historyIndex = index;
      return session.history[index].scroll;
    }
    if (session.history[session.historyIndex]?.path !== path) {
      session.history.splice(session.historyIndex + 1);
      session.history.push({ path, scroll: 0 });
      if (session.history.length > directoryHistoryLimit) session.history.shift();
      session.historyIndex = session.history.length - 1;
    }
    return 0;
  }

  private async navigate(session: FileSession, input: string, force = false, historyIndex?: number): Promise<void> {
    if (session.busy || !this.live(session)) return;
    let path: string;
    try { path = normalizeDevicePath(input); } catch (error) { session.error = message(error); this.update(session); return; }
    if (historyIndex != null && session.history[historyIndex]?.path !== path) return;
    this.saveHistoryScroll(session);
    const request = ++session.request;
    const version = session.cacheVersion;
    session.loading = true;
    session.selected = "";
    session.selectedPaths.clear(); session.selectionAnchor = "";
    session.selectedNavigation = "";
    session.error = "";
    this.update(session);
    try {
      const result = await this.readDirectory(session, path, force);
      if (!this.live(session) || request !== session.request || version !== session.cacheVersion) return;
      const scroll = this.commitDirectoryHistory(session, result.path, historyIndex);
      session.path = result.path;
      session.draftPath = result.path;
      session.entries = result.entries;
      session.directories.set(result.path, result.entries);
      session.treeErrors.delete(result.path);
      session.selected = "";
      session.selectedPaths.clear(); session.selectionAnchor = "";
      session.selectedNavigation = "";
      session.scroll = scroll;
      if (!session.syncing) session.message = "";
      // Restore visible ancestors after cache invalidation without expanding them.
      let ancestor = parentPath(result.path);
      while (ancestor !== result.path) {
        if (session.expanded.has(ancestor) && !session.directories.has(ancestor)) void this.loadTree(session, ancestor);
        if (ancestor === "/") break;
        ancestor = parentPath(ancestor);
      }
    } catch (error) {
      if (this.live(session) && request === session.request && version === session.cacheVersion) { session.error = message(error); session.draftPath = session.path; session.treeErrors.set(path, session.error); }
    } finally {
      if (request === session.request && this.live(session)) { session.loading = false; this.update(session); }
    }
  }

  private async loadTree(session: FileSession, path: string): Promise<void> {
    const version = session.cacheVersion;
    if (session.treeLoading.get(path) === version) return;
    session.treeLoading.set(path, version);
    this.update(session);
    try {
      const result = await this.readDirectory(session, path);
      if (!this.live(session) || version !== session.cacheVersion) return;
      session.directories.set(path, result.entries);
      session.treeErrors.delete(path);
    } catch (error) { if (version === session.cacheVersion) session.treeErrors.set(path, message(error)); }
    finally { if (session.treeLoading.get(path) === version) session.treeLoading.delete(path); if (this.live(session)) this.update(session); }
  }
  private async expand(session: FileSession, path: string): Promise<void> {
    if (session.expanded.has(path) && !session.treeErrors.has(path)) session.expanded.delete(path);
    else { session.expanded.add(path); if (!session.directories.has(path) || session.treeErrors.has(path) || Date.now() - (session.readAt.get(path) ?? 0) >= directoryCacheMs) await this.loadTree(session, path); }
    this.update(session);
  }

  private async openTreeDirectory(session: FileSession, path: string): Promise<void> {
    if (this.actionDisabled(session, "go")) return;
    const collapsed = session.expanded.delete(path);
    if (session.path !== path || session.error || session.treeErrors.has(path)) await this.navigate(session, path);
    else if (collapsed) this.update(session);
  }

  private openSelectedDirectory(session: FileSession): void {
    if (this.actionDisabled(session, "go")) return;
    const entry = session.entries.find(entry => entry.path === session.selected);
    if (session.selectedNavigation || entry?.is_dir) void this.navigate(session, session.selected);
  }

  private actionTarget(session: FileSession): FileActionTarget {
    return { directory: session.path, entry: session.selectedNavigation ? undefined : session.entries.find(entry => entry.path === session.selected),
      entries: session.selectedNavigation ? [] : session.entries.filter(entry => session.selectedPaths.has(entry.path)), multiple: session.selectedPaths.size > 1 };
  }

  private actionDisabled(session: FileSession, action: string, target = this.actionTarget(session)): boolean {
    if (!this.live(session) || !this.hooks.online(session.deviceId) || session.busy || session.loading) return true;
    if (["up", "go", "refresh"].includes(action)) return action === "up" && target.directory === "/";
    if (session.syncing || transferRunning(session)) return true;
    if (action === "copy") {
      const entries = target.entries ?? (target.entry ? [target.entry] : []);
      return !entries.length || entries.length > 256 || entries.some(entry => !["file", "directory"].includes(entry.kind));
    }
    if (action === "delete") return !(target.entries?.length || target.entry);
    if (["download", "rename"].includes(action)) return !!target.multiple || !target.entry || (action === "download" && target.entry.kind === "other");
    return false;
  }

  private beginMarquee(session: FileSession, list: HTMLElement, event: PointerEvent): void {
    const rect = list.getBoundingClientRect();
    if (event.clientX >= rect.left + list.clientWidth || event.clientY >= rect.top + list.clientHeight) return; // Scrollbars aren't selection space.
    this.endMarquee(); this.dragCandidate = null; this.pointerSelection = null;
    const body = list.querySelector("tbody");
    if (!body) return;
    this.marquee = { session, list, pointerId: event.pointerId,
      startX: event.clientX - rect.left + list.scrollLeft, startY: event.clientY - rect.top + list.scrollTop,
      clientX: event.clientX, clientY: event.clientY, rowTop: body.getBoundingClientRect().top - rect.top + list.scrollTop,
      base: new Set(session.selectedPaths), mode: event.ctrlKey || event.metaKey ? "toggle" : event.shiftKey ? "add" : "replace",
      active: false, overlay: null, frame: 0 };
    list.focus({ preventScroll: true });
  }

  private moveMarquee(event: PointerEvent): void {
    const marquee = this.marquee;
    if (!marquee || marquee.pointerId !== event.pointerId) return;
    if (!(event.buttons & 1)) { this.endMarquee(); return; }
    if (!marquee.active && Math.hypot(event.clientX - marquee.clientX, event.clientY - marquee.clientY) < 5) return;
    marquee.clientX = event.clientX; marquee.clientY = event.clientY;
    if (!marquee.active) {
      marquee.active = true;
      marquee.overlay = document.createElement("div");
      marquee.overlay.className = "device-files-marquee";
      marquee.overlay.setAttribute("aria-hidden", "true");
      document.body.append(marquee.overlay);
      try { marquee.list.setPointerCapture(marquee.pointerId); } catch { /* Synthetic tests don't own native pointers. */ }
      this.scrollMarquee();
    }
    event.preventDefault();
    this.paintMarquee();
  }

  private paintMarquee(): void {
    const marquee = this.marquee;
    if (!marquee?.active) return;
    const { list, session } = marquee;
    const rect = list.getBoundingClientRect();
    const header = list.querySelector("thead")!.getBoundingClientRect();
    const x = marquee.clientX - rect.left + list.scrollLeft;
    const y = marquee.clientY - rect.top + list.scrollTop;
    const top = Math.min(marquee.startY, y), bottom = Math.max(marquee.startY, y);
    const left = Math.min(marquee.startX, x), right = Math.max(marquee.startX, x);
    const tableWidth = list.querySelector("table")!.getBoundingClientRect().width;
    const first = Math.max(0, Math.floor((top - marquee.rowTop) / fileRowHeight) - 1);
    const end = Math.min(session.entries.length, Math.ceil((bottom - marquee.rowTop) / fileRowHeight) - 1);
    const hits = right > 0 && left < tableWidth && bottom > top ? session.entries.slice(first, Math.max(first, end)) : [];
    const selected = marquee.mode === "replace" ? new Set<string>() : new Set(marquee.base);
    for (const entry of hits) {
      if (marquee.mode === "toggle" && marquee.base.has(entry.path)) selected.delete(entry.path);
      else selected.add(entry.path);
    }
    session.selectedPaths = selected; session.selectedNavigation = "";
    session.selected = [...selected].pop() ?? "";
    session.selectionAnchor = hits[0]?.path ?? session.selected;
    this.paintSelection(session);
    const clipLeft = rect.left, clipRight = rect.left + list.clientWidth;
    const clipTop = Math.max(rect.top, header.bottom), clipBottom = rect.top + list.clientHeight;
    const displayLeft = Math.max(clipLeft, Math.min(clipRight, rect.left + left - list.scrollLeft));
    const displayTop = Math.max(clipTop, Math.min(clipBottom, rect.top + top - list.scrollTop));
    if (marquee.overlay) Object.assign(marquee.overlay.style, {
      left: `${displayLeft}px`, top: `${displayTop}px`,
      width: `${Math.max(0, Math.min(clipRight, rect.left + right - list.scrollLeft) - displayLeft)}px`,
      height: `${Math.max(0, Math.min(clipBottom, rect.top + bottom - list.scrollTop) - displayTop)}px`,
    });
  }

  private scrollMarquee(): void {
    const marquee = this.marquee;
    if (!marquee?.active) return;
    marquee.frame = requestAnimationFrame(() => {
      if (this.marquee !== marquee) return;
      if (!marquee.list.isConnected || this.hooks.activeId() !== marquee.session.id || !this.live(marquee.session)) { this.endMarquee(); return; }
      const { list } = marquee;
      const rect = list.getBoundingClientRect();
      const top = list.querySelector("thead")!.getBoundingClientRect().bottom;
      const speed = (position: number, low: number, high: number) => position < low + 24 ? -Math.min(24, (low + 24 - position) / 2) : position > high - 24 ? Math.min(24, (position - high + 24) / 2) : 0;
      const beforeX = list.scrollLeft, beforeY = list.scrollTop;
      list.scrollTop += speed(marquee.clientY, top, rect.top + list.clientHeight);
      list.scrollLeft += speed(marquee.clientX, rect.left, rect.left + list.clientWidth);
      if (beforeX !== list.scrollLeft || beforeY !== list.scrollTop) this.paintMarquee();
      this.scrollMarquee();
    });
  }

  private endMarquee(): void {
    const marquee = this.marquee;
    if (!marquee) return;
    this.marquee = null;
    if (marquee.active) this.suppressSelectionClick = marquee.session.id;
    cancelAnimationFrame(marquee.frame);
    marquee.overlay?.remove();
    if (marquee.list.hasPointerCapture(marquee.pointerId)) marquee.list.releasePointerCapture(marquee.pointerId);
  }

  private select(session: FileSession, path: string, navigation: FileSession["selectedNavigation"] = "", keys?: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): void {
    if (!path || navigation) {
      session.selected = path; session.selectedNavigation = navigation; session.selectedPaths.clear(); session.selectionAnchor = "";
    } else {
      const additive = keys?.ctrlKey || keys?.metaKey;
      const anchor = session.entries.findIndex(entry => entry.path === session.selectionAnchor);
      const index = session.entries.findIndex(entry => entry.path === path);
      if (keys?.shiftKey && anchor >= 0 && index >= 0) {
        if (!additive) session.selectedPaths.clear();
        for (const entry of session.entries.slice(Math.min(anchor, index), Math.max(anchor, index) + 1)) session.selectedPaths.add(entry.path);
      } else {
        if (!additive) session.selectedPaths.clear();
        if (additive && session.selectedPaths.has(path)) session.selectedPaths.delete(path); else session.selectedPaths.add(path);
        session.selectionAnchor = path;
      }
      session.selected = session.selectedPaths.has(path) ? path : [...session.selectedPaths].pop() ?? "";
      session.selectedNavigation = "";
    }
    this.paintSelection(session);
  }

  private paintSelection(session: FileSession): void {
    document.querySelectorAll<HTMLElement>("[data-file-path]").forEach(row => {
      const selected = row.dataset.fileNavigation ? row.dataset.filePath === session.selected && row.dataset.fileNavigation === session.selectedNavigation : !session.selectedNavigation && session.selectedPaths.has(row.dataset.filePath!);
      row.classList.toggle("selected", selected);
      row.setAttribute("aria-selected", String(selected));
    });
    const count = document.querySelector<HTMLElement>("[data-files-selection-count]");
    if (count) count.textContent = session.selectedPaths.size > 1 ? `已选 ${session.selectedPaths.size} 项` : "";
    for (const action of ["download", "rename", "delete"]) {
      const button = document.querySelector<HTMLButtonElement>(`[data-files-action="${action}"]`);
      if (button) button.disabled = this.actionDisabled(session, action);
    }
  }

  private closeContextMenu(): void {
    this.contextMenu?.remove();
    this.contextMenu = null;
    document.querySelectorAll(".device-files-tree-row.context-target").forEach(row => row.classList.remove("context-target"));
  }

  private openContextMenu(session: FileSession, target: FileActionTarget, x: number, y: number): void {
    this.closeContextMenu();
    const menu = document.createElement("div");
    menu.id = "device-files-context-menu";
    menu.className = "terminal-context-menu device-files-context-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "设备文件操作");
    menu.innerHTML = fileMenuActions.map(([action, label, icon]) => `<button type="button" role="menuitem" class="terminal-context-item"
      data-files-menu-action="${action}" ${this.actionDisabled(session, action, target) ? "disabled" : ""}>
      <span class="terminal-context-icon" aria-hidden="true">&#x${icon};</span><span>${escape(action === "sync" && session.syncing ? "正在同步…" : label)}</span></button>`).join("");
    document.body.append(menu);
    this.contextMenu = menu;
    const rect = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - rect.width - 8))}px`;
    menu.style.top = `${Math.max(8, Math.min(y, window.innerHeight - rect.height - 8))}px`;
    menu.addEventListener("click", event => {
      const button = (event.target as Element).closest<HTMLButtonElement>("[data-files-menu-action]");
      if (!button || button.disabled) return;
      const action = button.dataset.filesMenuAction!;
      this.closeContextMenu();
      if (session.id === this.hooks.activeId() && !this.actionDisabled(session, action, target)) void this.action(session, action, target);
    });
    menu.addEventListener("contextmenu", event => event.preventDefault());
    menu.addEventListener("keydown", event => {
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const buttons = [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
  }

  private async action(session: FileSession, action: string, target = this.actionTarget(session)): Promise<void> {
    if (action === "cancel-transfer") { await this.cancel(session); return; }
    if (action === "dismiss-transfer") { this.clearTransferTimer(session); session.transfer = null; this.update(session); return; }
    if (this.actionDisabled(session, action, target)) return;
    this.closeContextMenu();
    if (action === "refresh" || action === "up" || action === "go") {
      await this.navigate(session, action === "go" ? session.draftPath : action === "up" ? parentPath(target.directory) : target.directory, action === "refresh"); return;
    }
    if (action === "sync") { await this.sync(session); return; }
    const { entry, directory } = target;
    if (action === "copy") { await this.copyFiles(session, target.entries ?? (entry ? [entry] : [])); return; }
    if (action === "paste") { await this.prepareDroppedFiles(session, directory, null); return; }
    if (action.startsWith("upload-")) { await this.upload(session, action, directory); return; }
    if (action === "download" && entry) { await this.download(session, entry); return; }
    if (action === "mkdir") {
      this.dialog = { session, title: "新建目录", message: `${session.deviceId}\n${directory}`, input: "新建目录", accept: "创建", danger: false, error: "",
        submit: name => { void this.mutate(session, "mkdir", directory, name); } };
    } else if (action === "rename" && entry) {
      this.dialog = { session, title: "重命名", message: `${session.deviceId}\n${entry.path}`, input: entry.name, accept: "重命名", danger: false, error: "",
        submit: name => { void this.mutate(session, "rename", entry.path, name); } };
    } else if (action === "delete") {
      const entries = (target.entries ?? (entry ? [entry] : [])).map(item => ({ ...item }));
      if (!entries.length) return;
      const only = entries.length === 1 ? entries[0] : null;
      const paths = entries.slice(0, 20).map(item => item.path).join("\n") + (entries.length > 20 ? `\n…另有 ${entries.length - 20} 项` : "");
      this.dialog = { session, title: only ? only.is_dir ? "删除目录" : "删除文件" : `删除 ${entries.length} 个项目？`,
        message: `${session.deviceId}\n${paths}\n${entries.some(item => item.is_dir && item.kind !== "symlink") ? "选中目录及其内容将被删除。" : ""}此操作不可撤销。${only ? "" : "\n将依次删除，遇到失败会停止；已删除项目不会恢复。"}`,
        input: null, accept: only ? "删除" : `删除 ${entries.length} 项`, danger: true, error: "",
        submit: () => { if (only) void this.mutate(session, "delete", only.path, ""); else void this.deleteEntries(session, entries); } };
    }
    this.hooks.render();
  }

  private async sync(session: FileSession): Promise<void> {
    session.syncing = true;
    session.error = "";
    session.message = "正在将设备缓存同步到存储…";
    this.update(session);
    try {
      await invoke<void>("sync_device_files", { deviceId: session.deviceId });
      session.message = "缓存已同步到设备存储";
    } catch (error) { session.error = `同步失败：${message(error)}`; }
    finally { session.syncing = false; this.update(session); }
  }

  private async mutate(session: FileSession, action: string, path: string, name: string): Promise<void> {
    if (!this.live(session) || session.busy || session.syncing || transferRunning(session)) return;
    session.busy = true; session.error = "";
    this.update(session);
    try {
      await invoke<void>("mutate_device_file", { deviceId: session.deviceId, action, path, name, confirmed: action === "delete" });
      session.directories.clear();
      session.readAt.clear();
      ++session.cacheVersion;
      session.treeErrors.clear();
    } catch (error) { session.error = message(error); }
    finally { session.busy = false; }
    if (!session.error) await this.navigate(session, session.path);
    else this.update(session);
  }

  private async deleteEntries(session: FileSession, entries: DeviceFileEntry[]): Promise<void> {
    if (this.actionDisabled(session, "delete", { directory: session.path, entries })) return;
    session.busy = true; session.error = "";
    let completed = 0;
    let failure = "";
    try {
      for (const entry of entries) {
        if (!this.live(session)) return;
        session.message = `正在删除 ${completed + 1}/${entries.length}：${entry.name}`; this.update(session);
        await invoke<void>("mutate_device_file", { deviceId: session.deviceId, action: "delete", path: entry.path, name: "", confirmed: true });
        ++completed;
      }
    } catch (error) { failure = message(error); }
    finally { session.busy = false; }
    if (!this.live(session)) return;
    session.directories.clear(); session.readAt.clear(); session.treeErrors.clear(); ++session.cacheVersion;
    await this.navigate(session, session.path);
    if (failure) {
      session.error = `已删除 ${completed}/${entries.length} 项，停止于 ${entries[completed].name}：${failure}；剩余 ${entries.length - completed} 项未删除。`;
      const remaining = new Set(entries.slice(completed).map(entry => entry.path));
      session.selectedPaths = new Set(session.entries.filter(entry => remaining.has(entry.path)).map(entry => entry.path));
      session.selected = [...session.selectedPaths][0] ?? ""; session.selectionAnchor = session.selected;
    } else session.message = `已删除 ${completed} 个项目`;
    this.update(session);
  }

  private async upload(session: FileSession, mode: string, directory: string): Promise<void> {
    session.busy = true; session.error = ""; this.update(session);
    let local: LocalSelection | null = null;
    let listing: DirectoryListing | null = null;
    try {
      local = await invoke<LocalSelection | null>("choose_device_local_path", { mode, name: "" });
      if (local) listing = await invoke<DirectoryListing>("list_device_directory", { deviceId: session.deviceId, path: directory });
    } catch (error) { session.error = message(error); }
    finally { session.busy = false; }
    if (!local || !listing || !this.live(session)) { this.update(session); return; }
    const target = `${directory === "/" ? "" : directory}/${local.name}`;
    const existing = listing.entries.find(entry => entry.name === local!.name);
    if (existing) {
      if (existing.kind !== "file" || local.is_dir) { session.error = "同名项目已存在，目录和符号链接不覆盖；请先重命名。"; this.update(session); return; }
      this.dialog = { session, title: "覆盖设备文件？", message: `${session.deviceId}\n${target}\n上传完成后将替换原文件。`, input: null, accept: "覆盖", danger: true, error: "",
        submit: () => { void this.startTransfer(session, "upload", target, local!.path, true); } };
      this.hooks.render();
    } else await this.startTransfer(session, "upload", target, local.path, false);
  }

  private clearDropTarget(): void {
    this.dropTarget = null;
    document.querySelector(".device-files-list.drop-active")?.classList.remove("drop-active");
  }

  private onDragDrop(event: DragDropEvent): void {
    if (event.type === "leave") { this.clearDropTarget(); return; }
    const session = this.sessions.get(this.hooks.activeId());
    const list = document.querySelector<HTMLElement>(".device-files-list");
    const rect = list?.getBoundingClientRect();
    const x = event.position.x / window.devicePixelRatio, y = event.position.y / window.devicePixelRatio;
    const valid = session && rect && x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
      && !this.dialog && !this.actionDisabled(session, "upload-file") && !session.dragging;
    if (!valid) { this.clearDropTarget(); return; }
    if (event.type !== "drop") {
      this.dropTarget = { session, directory: session.path };
      list?.classList.add("drop-active");
      return;
    }
    const target = this.dropTarget ?? { session, directory: session.path };
    this.clearDropTarget();
    if (target.session !== session || target.directory !== session.path) return;
    void this.prepareDroppedFiles(session, target.directory, event.paths);
  }

  private async copyFiles(session: FileSession, entries: DeviceFileEntry[]): Promise<void> {
    if (this.dialog || !entries.length) return;
    const remotePaths = entries.map(entry => entry.path);
    const remoteDirectories = entries.filter(entry => entry.kind === "directory").map(entry => entry.path);
    const id = crypto.randomUUID();
    session.clipboardId = id;
    session.busy = true; session.error = ""; session.message = remoteDirectories.length ? "正在读取目录结构，准备复制…" : ""; this.update(session);
    try {
      await this.ready;
      await invoke<void>("copy_device_files", { id, deviceId: session.deviceId, remotePaths, remoteDirectories });
      if (this.live(session) && session.clipboardId === id) {
        session.message = `已复制 ${entries.length} 个${remoteDirectories.length ? "项目（含文件夹）" : "文件"}，请到电脑文件夹或桌面按 Ctrl+V 下载（粘贴完成前保持软件运行）。`;
      }
    } catch (error) { session.error = message(error); session.message = ""; }
    finally { session.busy = false; this.update(session); }
  }

  private async prepareDroppedFiles(session: FileSession, directory: string, paths: string[] | null): Promise<void> {
    if (this.dialog || this.actionDisabled(session, "upload-file")) return;
    session.busy = true; session.error = ""; this.closeContextMenu(); this.update(session);
    try {
      const localPaths = paths ?? await invoke<string[]>("read_device_clipboard_files");
      if (!localPaths.length) throw new Error("剪贴板中没有可上传的文件，请先在 Windows 资源管理器中复制文件。");
      const locals = await invoke<LocalSelection[]>("inspect_device_upload_paths", { paths: localPaths, allowDirectories: paths === null });
      if (!this.live(session)) return;
      if (this.dialog || (paths === null && this.hooks.activeId() !== session.id)) {
        session.message = "已取消粘贴，请回到目标文件页后重试。"; return;
      }
      const items = locals.map(local => ({ local, name: local.name }));
      this.uploadNamesDialog(session, directory, items);
    } catch (error) { session.error = message(error); }
    finally { session.busy = false; this.update(session); }
  }

  private uploadNamesDialog(session: FileSession, directory: string, items: UploadItem[], error = ""): void {
    this.dialog = { session, title: `确认上传名称（${items.length} 个${items.some(item => item.local.is_dir) ? "项目" : "文件"}）`, message: `${session.deviceId}\n上传到：${directory}\n可修改每个文件或文件夹名称；文件夹包含其子目录，已有目录不合并覆盖；取消不会上传任何项目。`,
      input: null, uploads: items, accept: "确认上传", danger: false, error,
      submit: () => { void this.confirmDroppedFiles(session, directory, items); } };
    this.hooks.render();
  }

  private async confirmDroppedFiles(session: FileSession, directory: string, items: UploadItem[]): Promise<void> {
    if (this.actionDisabled(session, "upload-file")) return;
    session.busy = true; session.error = ""; this.update(session);
    try {
      // Never authorize overwriting from the browse cache.
      const listing = await invoke<DirectoryListing>("list_device_directory", { deviceId: session.deviceId, path: directory });
      if (!this.live(session)) return;
      const existing = new Map(listing.entries.map(entry => [entry.name, entry]));
      const conflicts = items.filter(item => existing.has(item.name));
      if (conflicts.some(item => item.local.is_dir || existing.get(item.name)!.kind !== "file")) {
        this.uploadNamesDialog(session, directory, items, "涉及目录或符号链接的同名项目不能覆盖，请修改对应上传名称。"); return;
      }
      const start = () => { void this.uploadBatch(session, directory, items, new Set(conflicts.map(item => item.name))); };
      if (conflicts.length) {
        this.dialog = { session, title: "覆盖设备文件？", message: `${session.deviceId}\n${directory}\n以下 ${conflicts.length} 个同名文件将被替换：\n${conflicts.map(item => item.name).join("\n")}\n取消则整批不上传。`,
          input: null, accept: "覆盖并上传", danger: true, error: "", submit: start };
        this.hooks.render();
      } else {
        session.busy = false;
        start();
      }
    } catch (error) { session.error = message(error); }
    finally { session.busy = false; this.update(session); }
  }

  private async uploadBatch(session: FileSession, directory: string, items: UploadItem[], overwrites: Set<string>): Promise<void> {
    if (this.actionDisabled(session, "upload-file")) return;
    const batch = { cancelled: false };
    session.batch = batch;
    let completed = 0;
    for (const item of items) {
      if (batch.cancelled || !this.live(session)) break;
      session.message = `正在上传 ${completed + 1}/${items.length}：${item.name}`;
      const target = `${directory === "/" ? "" : directory}/${item.name}`;
      if (!await this.startTransfer(session, "upload", target, item.local.path, overwrites.has(item.name))) break;
      ++completed;
    }
    session.batch = null;
    session.cancelling = false;
    const summary = `${batch.cancelled ? "已取消后续上传" : completed === items.length ? "上传完成" : "上传中止"}：${completed}/${items.length} 个${items.some(item => item.local.is_dir) ? "项目" : "文件"}`;
    if (session.transfer) session.transfer = { ...session.transfer, phase: batch.cancelled ? "cancelled" : completed === items.length ? "completed" : "failed", message: `${summary}${session.transfer.phase === "failed" ? `；${session.transfer.message}` : ""}` };
    this.scheduleTransferDismiss(session);
    session.message = summary;
    if (this.closing.delete(session.id)) { this.remove(session); return; }
    if (completed) {
      session.directories.clear(); session.readAt.clear(); ++session.cacheVersion;
      await this.navigate(session, session.path);
    }
    this.update(session);
  }

  private async dragDownload(session: FileSession, entries: DeviceFileEntry[]): Promise<void> {
    if (this.dialog || !entries.length || this.actionDisabled(session, "download", { directory: session.path, entry: entries[0] })) return;
    if (entries.length > 256 || entries.some(entry => entry.kind !== "file" || entry.is_dir)) {
      session.error = "请选择 1–256 个普通文件拖出，目录或符号链接请使用下载按钮。"; this.update(session); return;
    }
    this.closeContextMenu(); this.clearDropTarget();
    const id = crypto.randomUUID();
    this.clearTransferTimer(session);
    session.dragging = true; session.error = ""; session.cancelling = false;
    session.transfer = { id, device_id: session.deviceId, phase: "dragging", message: `拖到电脑文件夹或桌面：${entries.length === 1 ? entries[0].name : `${entries.length} 个文件`}（Esc 取消）`, percent: null, bytes: 0, total: null, elapsed_ms: 0 };
    this.update(session);
    try {
      await this.ready;
      if (session.cancelling) throw new Error("已取消传输");
      await invoke<void>("drag_device_file", { id, deviceId: session.deviceId, remotePaths: entries.map(entry => entry.path) });
      if (session.transfer?.id === id && ["dragging", "running"].includes(session.transfer.phase)) this.onProgress({ ...session.transfer, phase: "completed", percent: 100, message: `已复制到目标文件夹（${entries.length} 个文件）` });
    } catch (error) {
      if (session.transfer?.id === id && ["dragging", "running"].includes(session.transfer.phase)) this.onProgress({ ...session.transfer, phase: message(error).startsWith("已取消传输") ? "cancelled" : "failed", message: message(error) });
    } finally { session.dragging = false; this.update(session); }
  }

  private async download(session: FileSession, entry: DeviceFileEntry): Promise<void> {
    session.busy = true; session.error = ""; this.update(session);
    let local: LocalSelection | null = null;
    try { local = await invoke<LocalSelection | null>("choose_device_local_path", { mode: entry.is_dir ? "download-directory" : "download-file", name: entry.name }); }
    catch (error) { session.error = message(error); }
    finally { session.busy = false; }
    if (!local || !this.live(session)) { this.update(session); return; }
    if (local.exists) {
      if (entry.is_dir || local.is_dir) { session.error = "本地同名目录已存在，请选择其他保存位置；第一版不合并覆盖目录。"; this.update(session); return; }
      this.dialog = { session, title: "覆盖本地文件？", message: `${local.path}\n下载完成后将替换原文件，失败或取消不会覆盖。`, input: null, accept: "覆盖", danger: true, error: "",
        submit: () => { void this.startTransfer(session, "download", entry.path, local!.path, true); } };
      this.hooks.render();
    } else await this.startTransfer(session, "download", entry.path, local.path, false);
  }

  private async startTransfer(session: FileSession, direction: string, remotePath: string, localPath: string, overwrite: boolean): Promise<boolean> {
    if (!this.live(session) || session.dragging || session.transfer?.phase === "running") return false;
    const id = crypto.randomUUID();
    this.clearTransferTimer(session);
    session.error = ""; session.cancelling = false;
    session.transfer = { id, device_id: session.deviceId, phase: "running", message: direction === "upload" ? "准备上传…" : "准备下载…", percent: null, bytes: 0, total: null, elapsed_ms: 0 };
    this.update(session);
    try {
      await this.ready;
      if (session.cancelling || session.batch?.cancelled) throw new Error("已取消传输");
      await invoke<void>("transfer_device_file", { request: { id, device_id: session.deviceId, direction, remote_path: remotePath, local_path: localPath, overwrite } });
      if (session.transfer?.id === id && session.transfer.phase === "running") this.onProgress({ ...session.transfer, phase: "completed", percent: 100, message: "传输完成" });
    } catch (error) {
      if (session.transfer?.id === id && session.transfer.phase === "running") this.onProgress({ ...session.transfer, phase: message(error).startsWith("已取消传输") ? "cancelled" : "failed", message: message(error) });
    }
    return session.transfer?.id === id && session.transfer.phase === "completed";
  }

  private onProgress(progress: TransferProgress): void {
    const session = this.sessions.get(`device-files:${progress.device_id}`);
    if (!session || session.transfer?.id !== progress.id || !["dragging", "running"].includes(session.transfer.phase)) return;
    if (progress.phase === "dragging" && session.transfer.phase !== "dragging") return;
    if (session.cancelling && ["dragging", "running"].includes(progress.phase)) {
      void invoke("cancel_device_transfer", { id: progress.id }).catch(() => {});
      return; // Freeze the last visible progress while cancellation is being acknowledged.
    }
    const enteringTransfer = session.transfer.phase === "dragging" && progress.phase === "running";
    session.transfer = progress;
    if (progress.phase !== "running" && progress.phase !== "dragging") {
      session.cancelling = false;
      if (session.batch) { this.update(session); return; }
      if (this.closing.delete(session.id)) { this.remove(session); return; }
      this.scheduleTransferDismiss(session);
      if (progress.phase === "completed" && !session.dragging) { session.directories.clear(); session.readAt.clear(); ++session.cacheVersion; void this.navigate(session, session.path); }
      this.update(session);
      return;
    }
    // Do not rebuild the app on every progress tick: keep focus, scroll and terminal DOM intact.
    if (enteringTransfer) this.update(session);
    if (session.id !== this.hooks.activeId()) return;
    const label = document.querySelector<HTMLElement>("[data-files-transfer-message]");
    if (label) { label.textContent = progress.message; label.title = progress.message; }
    const detail = document.querySelector<HTMLElement>("[data-files-transfer-detail]");
    if (detail) detail.textContent = [progress.percent != null ? `${progress.percent}%` : "", progress.bytes ? sizeText(progress.bytes) : ""].filter(Boolean).join(" · ");
    const bar = document.querySelector<HTMLProgressElement>(".device-files-transfer progress");
    if (bar) { if (progress.percent == null) bar.removeAttribute("value"); else { bar.max = 100; bar.value = progress.percent; } }
  }

  private clearTransferTimer(session: FileSession): void {
    const timer = this.transferTimers.get(session.id);
    if (timer != null) clearTimeout(timer);
    this.transferTimers.delete(session.id);
  }

  private scheduleTransferDismiss(session: FileSession): void {
    this.clearTransferTimer(session);
    const transfer = session.transfer;
    if (!transfer || session.batch || ["dragging", "running"].includes(transfer.phase)) return;
    const timer = setTimeout(() => {
      if (this.transferTimers.get(session.id) !== timer) return;
      this.transferTimers.delete(session.id);
      if (!this.live(session) || session.transfer?.id !== transfer.id || session.batch || ["dragging", "running"].includes(session.transfer.phase)) return;
      session.transfer = null;
      this.update(session);
    }, transferNoticeMs);
    this.transferTimers.set(session.id, timer);
  }

  private async cancel(session: FileSession): Promise<void> {
    if (!session.transfer || !transferRunning(session)) return;
    if (session.batch) session.batch.cancelled = true;
    session.cancelling = true; this.update(session);
    try { await invoke<void>("cancel_device_transfer", { id: session.transfer.id }); }
    catch (error) { session.error = message(error); session.cancelling = false; this.update(session); }
  }
  private async close(id: string): Promise<void> {
    const session = this.sessions.get(id);
    if (!session) return;
    if (session.busy || session.syncing) { session.error = "文件操作或同步进行中，请稍后关闭此标签。"; this.update(session); return; }
    if (transferRunning(session)) {
      this.dialog = { session, title: "关闭文件标签？", message: "关闭此标签会取消正在进行的传输，已完成的文件操作不会撤销。", input: null, accept: "取消传输并关闭", danger: true, error: "",
        submit: () => { this.closing.add(session.id); void this.cancel(session); } };
      this.hooks.render();
    } else this.remove(session);
  }
  private remove(session: FileSession): void {
    if (this.marquee?.session === session) this.endMarquee();
    this.clearTransferTimer(session);
    ++session.request;
    this.sessions.delete(session.id);
    if (this.hooks.activeId() === session.id) this.hooks.activate("serial");
    else this.hooks.render();
  }
}
