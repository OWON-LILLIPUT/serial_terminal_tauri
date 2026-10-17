import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
interface TransferProgress {
  id: string;
  device_id: string;
  phase: "running" | "completed" | "failed" | "cancelled";
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
  entries: DeviceFileEntry[];
  directories: Map<string, DeviceFileEntry[]>;
  readAt: Map<string, number>;
  pendingReads: Map<string, { version: number; promise: Promise<DirectoryListing> }>;
  expanded: Set<string>;
  treeLoading: Map<string, number>;
  treeErrors: Map<string, string>;
  selected: string;
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
}
interface FileHooks {
  activeId(): string;
  activate(id: string): void;
  render(): void;
  online(deviceId: string): boolean;
}
interface FileActionTarget { directory: string; entry?: DeviceFileEntry }
const fileMenuActions = [
  ["up", "上一级", "E74A"], ["go", "前往", "E8A7"], ["refresh", "刷新", "E72C"],
  ["upload-file", "上传文件", "E898"], ["upload-directory", "上传目录", "E8B7"],
  ["download", "下载", "E896"], ["mkdir", "新建目录", "E8F4"],
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
function transferRunning(session: FileSession): boolean { return session.transfer?.phase === "running"; }
const directoryCacheMs = 30000;
const fileRowHeight = 32;
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
  private focus: { id: string; start: number | null; end: number | null } | null = null;

  constructor(private hooks: FileHooks) {
    this.ready = listen<TransferProgress>("device-file-transfer", event => this.onProgress(event.payload)).then(() => {});
    // Report listener errors on the file page rather than creating an unhandled rejection.
    void this.ready.catch(() => {});
    document.addEventListener("pointerdown", event => {
      if (this.contextMenu && !(event.target instanceof Node && this.contextMenu.contains(event.target))) this.closeContextMenu();
    });
    document.addEventListener("keydown", event => {
      if (!this.contextMenu || !["Escape", "Tab"].includes(event.key)) return;
      this.closeContextMenu();
      if (event.key === "Escape") { event.preventDefault(); document.querySelector<HTMLElement>(".device-files-list")?.focus(); }
    });
    document.addEventListener("scroll", event => {
      if (this.contextMenu && !(event.target instanceof Node && this.contextMenu.contains(event.target))) this.closeContextMenu();
    }, true);
    window.addEventListener("resize", () => this.closeContextMenu());
    window.addEventListener("blur", () => this.closeContextMenu());
  }

  has(id: string): boolean { return this.sessions.has(id); }
  open(deviceId: string): void {
    const id = `device-files:${deviceId}`;
    let session = this.sessions.get(id);
    if (!session) {
      session = {
        id, deviceId, path: "/", draftPath: "/", entries: [], directories: new Map(), readAt: new Map(), pendingReads: new Map(),
        expanded: new Set(["/"]), treeLoading: new Map(), treeErrors: new Map(), selected: "", selectedNavigation: "",
        loading: false, busy: false, request: 0, error: "", message: "", scroll: 0, treeScroll: 0,
        transfer: null, cancelling: false, cacheVersion: 0, lastOnline: this.hooks.online(deviceId), syncing: false, viewportHeight: 600,
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
        <div class="device-files-list" tabindex="0" aria-label="设备文件列表">
          <table class="device-files-table"><colgroup><col class="device-files-name-column"><col class="device-files-size-column"><col class="device-files-modified-column"><col class="device-files-permissions-column"></colgroup>
            <thead><tr><th>名称</th><th>大小</th><th>修改时间</th><th>权限</th></tr></thead>
            <tbody>${this.renderRows(session)}</tbody></table>
          ${session.loading ? '<div class="device-files-empty">正在读取目录…</div>' : !session.entries.length && !session.error ? '<div class="device-files-empty">此目录为空</div>' : ""}
        </div>
      </div>
      <div class="device-files-footer"><span class="${session.error ? "error-text" : ""}" title="${escape(session.error || session.message || session.path)}">${escape(session.error || session.message || `${session.entries.length} 个项目 · ${session.path}`)}</span></div>
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
      const selected = entry.path === session.selected && navigation === session.selectedNavigation;
      const modified = navigation ? "—" : modifiedText(entry.modified);
      return `<tr data-file-path="${escape(entry.path)}" data-file-navigation="${navigation}" class="${selected ? "selected" : ""}" aria-selected="${selected}">
        <td title="${escape(entry.path)}"><span class="device-file-icon ${entry.is_dir ? "folder" : ""}">${entry.is_dir ? "▸" : entry.kind === "symlink" ? "↗" : "▤"}</span>${escape(entry.name)}${navigation ? '<span class="device-file-link">上级目录</span>' : entry.kind === "symlink" ? '<span class="device-file-link">链接</span>' : ""}</td>
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
    const detail = [transfer.percent != null ? `${transfer.percent}%` : "", transfer.bytes ? sizeText(transfer.bytes) : "", `${(transfer.elapsed_ms / 1000).toFixed(0)} 秒`].filter(Boolean).join(" · ");
    return `<div class="device-files-transfer" aria-live="polite">
      <div class="device-files-transfer-text"><span data-files-transfer-message class="${transfer.phase === "failed" ? "error-text" : ""}" title="${escape(transfer.message)}">${escape(transfer.message)}</span><span data-files-transfer-detail>${escape(detail)}</span></div>
      <progress ${transfer.percent != null ? `value="${transfer.percent}" max="100"` : ""} aria-label="文件传输进度"></progress>
      ${transfer.phase === "running" ? `<button class="secondary-button" data-files-action="cancel-transfer" type="button" ${session.cancelling ? "disabled" : ""}>${session.cancelling ? "正在取消…" : "取消传输"}</button>` : '<button class="secondary-button" data-files-action="dismiss-transfer" type="button">收起</button>'}
    </div>`;
  }

  renderDialog(): string {
    const dialog = this.dialog;
    if (!dialog) return "";
    return `<div class="device-files-dialog-backdrop"><form id="device-files-dialog" class="device-files-dialog" role="dialog" aria-modal="true" aria-labelledby="device-files-dialog-title">
      <div id="device-files-dialog-title" class="device-files-dialog-title">${escape(dialog.title)}</div>
      <div class="device-files-dialog-message">${escape(dialog.message)}</div>
      ${dialog.input != null ? `<input id="device-files-dialog-input" class="text-input" aria-label="名称" value="${escape(dialog.input)}" maxlength="255" autocomplete="off" />` : ""}
      ${dialog.error ? `<div class="error-text">${escape(dialog.error)}</div>` : ""}
      <div class="device-files-dialog-actions"><button class="secondary-button" id="device-files-dialog-cancel" type="button">取消</button>
        <button class="secondary-button ${dialog.danger ? "danger-button" : ""}" type="submit">${escape(dialog.accept)}</button></div>
    </form></div>`;
  }

  bind(): void {
    this.closeContextMenu();
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
          const row = (event.target as Element)?.closest<HTMLElement>("[data-file-path]");
          if (!row) return;
          this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"]);
          list?.focus();
        });
      list?.addEventListener("dblclick", event => {
          const row = (event.target as Element)?.closest<HTMLElement>("[data-file-path]");
          if (!row) return;
          this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"]);
          this.openSelectedDirectory(session);
        });
      list?.addEventListener("keydown", event => {
        if (event.key === "Enter") {
          event.preventDefault(); this.openSelectedDirectory(session);
        } else if (event.key === "Delete") {
          event.preventDefault(); void this.action(session, "delete");
        }
      });
      document.querySelectorAll<HTMLElement>("[data-files-directory]").forEach(button => button.addEventListener("click", () => { void this.openTreeDirectory(session, button.dataset.filesDirectory!); }));
      document.querySelectorAll<HTMLElement>("[data-files-expand]").forEach(button => button.addEventListener("click", () => { void this.expand(session, button.dataset.filesExpand!); }));
      document.querySelector(".device-files")?.addEventListener("contextmenu", event => {
        const mouse = event as MouseEvent;
        const target = mouse.target as Element;
        if (target.closest("input, textarea") || this.dialog) return;
        mouse.preventDefault(); mouse.stopPropagation();
        const row = target.closest<HTMLElement>("[data-file-path]");
        const treeRow = target.closest<HTMLElement>(".device-files-tree-row");
        if (row) this.select(session, row.dataset.filePath!, row.dataset.fileNavigation as FileSession["selectedNavigation"]);
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
        const value = document.querySelector<HTMLInputElement>("#device-files-dialog-input")?.value ?? "";
        if (dialog.input != null && !validDeviceName(value)) { dialog.error = "名称不能为空，不能包含 /、控制字符，也不能是 . 或 ..（最多 255 字节）"; this.hooks.render(); return; }
        this.dialog = null;
        // Local panel redraws must not leave an already accepted modal behind.
        this.hooks.render();
        dialog.submit(value);
      });
      const input = document.querySelector<HTMLInputElement>("#device-files-dialog-input");
      input?.addEventListener("input", () => { if (this.dialog) this.dialog.input = input.value; });
      (input ?? form.querySelector<HTMLButtonElement>("#device-files-dialog-cancel"))?.focus();
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
      if (input instanceof HTMLInputElement) {
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

  private async navigate(session: FileSession, input: string, force = false): Promise<void> {
    if (session.busy || !this.live(session)) return;
    let path: string;
    try { path = normalizeDevicePath(input); } catch (error) { session.error = message(error); this.update(session); return; }
    const request = ++session.request;
    const version = session.cacheVersion;
    session.loading = true;
    session.selected = "";
    session.selectedNavigation = "";
    session.error = "";
    this.update(session);
    try {
      const result = await this.readDirectory(session, path, force);
      if (!this.live(session) || request !== session.request || version !== session.cacheVersion) return;
      session.path = result.path;
      session.draftPath = result.path;
      session.entries = result.entries;
      session.directories.set(result.path, result.entries);
      session.treeErrors.delete(result.path);
      session.expanded.add(result.path);
      session.selected = "";
      session.selectedNavigation = "";
      session.scroll = 0;
      if (!session.syncing) session.message = "";
      // Ensure typed deep paths have ancestors in the lazy directory tree.
      let ancestor = parentPath(result.path);
      while (ancestor !== result.path) {
        session.expanded.add(ancestor);
        if (!session.directories.has(ancestor)) void this.loadTree(session, ancestor);
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
    if (session.path === path && session.expanded.has(path) && !session.treeErrors.has(path)) {
      session.expanded.delete(path);
      this.update(session);
    } else await this.navigate(session, path);
  }

  private openSelectedDirectory(session: FileSession): void {
    if (this.actionDisabled(session, "go")) return;
    const entry = session.entries.find(entry => entry.path === session.selected);
    if (session.selectedNavigation || entry?.is_dir) void this.navigate(session, session.selected);
  }

  private actionTarget(session: FileSession): FileActionTarget {
    return { directory: session.path, entry: session.selectedNavigation ? undefined : session.entries.find(entry => entry.path === session.selected) };
  }

  private actionDisabled(session: FileSession, action: string, target = this.actionTarget(session)): boolean {
    if (!this.live(session) || !this.hooks.online(session.deviceId) || session.busy || session.loading) return true;
    if (["up", "go", "refresh"].includes(action)) return action === "up" && target.directory === "/";
    if (session.syncing || transferRunning(session)) return true;
    if (["download", "rename", "delete"].includes(action)) return !target.entry || (action === "download" && target.entry.kind === "other");
    return false;
  }

  private select(session: FileSession, path: string, navigation: FileSession["selectedNavigation"] = ""): void {
    session.selected = path;
    session.selectedNavigation = navigation;
    document.querySelectorAll<HTMLElement>("[data-file-path]").forEach(row => {
      const selected = row.dataset.filePath === path && (row.dataset.fileNavigation || "") === navigation;
      row.classList.toggle("selected", selected);
      row.setAttribute("aria-selected", String(selected));
    });
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
    if (action === "dismiss-transfer") { session.transfer = null; this.update(session); return; }
    if (this.actionDisabled(session, action, target)) return;
    this.closeContextMenu();
    if (action === "refresh" || action === "up" || action === "go") {
      await this.navigate(session, action === "go" ? session.draftPath : action === "up" ? parentPath(target.directory) : target.directory, action === "refresh"); return;
    }
    if (action === "sync") { await this.sync(session); return; }
    const { entry, directory } = target;
    if (action.startsWith("upload-")) { await this.upload(session, action, directory); return; }
    if (action === "download" && entry) { await this.download(session, entry); return; }
    if (action === "mkdir") {
      this.dialog = { session, title: "新建目录", message: `${session.deviceId}\n${directory}`, input: "新建目录", accept: "创建", danger: false, error: "",
        submit: name => { void this.mutate(session, "mkdir", directory, name); } };
    } else if (action === "rename" && entry) {
      this.dialog = { session, title: "重命名", message: `${session.deviceId}\n${entry.path}`, input: entry.name, accept: "重命名", danger: false, error: "",
        submit: name => { void this.mutate(session, "rename", entry.path, name); } };
    } else if (action === "delete" && entry) {
      this.dialog = { session, title: entry.is_dir ? "删除目录" : "删除文件", message: `${session.deviceId}\n${entry.path}\n${entry.is_dir && entry.kind !== "symlink" ? "目录及其内容将被删除。" : ""}此操作不可撤销。`, input: null, accept: "删除", danger: true, error: "",
        submit: () => { void this.mutate(session, "delete", entry.path, ""); } };
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

  private async startTransfer(session: FileSession, direction: string, remotePath: string, localPath: string, overwrite: boolean): Promise<void> {
    if (!this.live(session) || transferRunning(session)) return;
    const id = crypto.randomUUID();
    session.error = ""; session.cancelling = false;
    session.transfer = { id, device_id: session.deviceId, phase: "running", message: direction === "upload" ? "准备上传…" : "准备下载…", percent: null, bytes: 0, total: null, elapsed_ms: 0 };
    this.update(session);
    try {
      await this.ready;
      await invoke<void>("transfer_device_file", { request: { id, device_id: session.deviceId, direction, remote_path: remotePath, local_path: localPath, overwrite } });
      if (session.transfer?.id === id && session.transfer.phase === "running") this.onProgress({ ...session.transfer, phase: "completed", percent: 100, message: "传输完成" });
    } catch (error) {
      if (session.transfer?.id === id && session.transfer.phase === "running") this.onProgress({ ...session.transfer, phase: message(error).startsWith("已取消传输") ? "cancelled" : "failed", message: message(error) });
    }
  }

  private onProgress(progress: TransferProgress): void {
    const session = this.sessions.get(`device-files:${progress.device_id}`);
    if (!session || session.transfer?.id !== progress.id || session.transfer.phase !== "running") return;
    session.transfer = progress;
    if (session.cancelling && progress.phase === "running") void invoke("cancel_device_transfer", { id: progress.id }).catch(() => {});
    if (progress.phase !== "running") {
      session.cancelling = false;
      if (this.closing.delete(session.id)) { this.remove(session); return; }
      if (progress.phase === "completed") { session.directories.clear(); session.readAt.clear(); ++session.cacheVersion; void this.navigate(session, session.path); }
      this.update(session);
      return;
    }
    // Do not rebuild the app on every progress tick: keep focus, scroll and terminal DOM intact.
    if (session.id !== this.hooks.activeId()) return;
    const label = document.querySelector<HTMLElement>("[data-files-transfer-message]");
    if (label) { label.textContent = progress.message; label.title = progress.message; }
    const detail = document.querySelector<HTMLElement>("[data-files-transfer-detail]");
    if (detail) detail.textContent = [progress.percent != null ? `${progress.percent}%` : "", progress.bytes ? sizeText(progress.bytes) : "", `${(progress.elapsed_ms / 1000).toFixed(0)} 秒`].filter(Boolean).join(" · ");
    const bar = document.querySelector<HTMLProgressElement>(".device-files-transfer progress");
    if (bar) { if (progress.percent == null) bar.removeAttribute("value"); else { bar.max = 100; bar.value = progress.percent; } }
  }

  private async cancel(session: FileSession): Promise<void> {
    if (!session.transfer || !transferRunning(session)) return;
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
    ++session.request;
    this.sessions.delete(session.id);
    if (this.hooks.activeId() === session.id) this.hooks.activate("serial");
    else this.hooks.render();
  }
}
