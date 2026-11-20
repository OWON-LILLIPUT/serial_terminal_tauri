use crate::{
    adb_shell_protocol::TerminalSize,
    app::{device_state, Action, App, Modal, Tab},
    files::Entry,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tui_term::widget::{Cursor, PseudoTerminal};

const BG: Color = Color::Rgb(30, 30, 46);
const SIDE: Color = Color::Rgb(24, 24, 37);
const SURFACE: Color = Color::Rgb(49, 50, 68);
const HIGHLIGHT: Color = Color::Rgb(69, 71, 90);
const TEXT: Color = Color::Rgb(205, 214, 244);
const MUTED: Color = Color::Rgb(166, 173, 200);
const BLUE: Color = Color::Rgb(137, 180, 250);
const GREEN: Color = Color::Rgb(166, 227, 161);
const RED: Color = Color::Rgb(243, 139, 168);
const TERMINAL_SELECTION: Color = Color::Rgb(88, 91, 112);
const TERMINAL_CURSOR: Color = Color::Rgb(245, 224, 220);
const TERMINAL_ANSI: [Color; 16] = [
    HIGHLIGHT,
    RED,
    GREEN,
    Color::Rgb(249, 226, 175),
    BLUE,
    Color::Rgb(245, 194, 231),
    Color::Rgb(148, 226, 213),
    Color::Rgb(186, 194, 222),
    TERMINAL_SELECTION,
    RED,
    GREEN,
    Color::Rgb(249, 226, 175),
    BLUE,
    Color::Rgb(245, 194, 231),
    Color::Rgb(148, 226, 213),
    MUTED,
];

#[derive(Clone, Copy)]
pub struct Geometry {
    area: Rect,
    top: Rect,
    side: Rect,
    tabs: Rect,
    body: Rect,
    footer: Rect,
    pub terminal: Rect,
}

pub fn geometry(area: Rect) -> Geometry {
    let [top, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let width = (area.width / 3).clamp(21, 39).min(area.width / 2);
    let [side, right] =
        Layout::horizontal([Constraint::Length(width), Constraint::Min(0)]).areas(main);
    let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(right);
    let terminal = Block::bordered().inner(body);
    Geometry {
        area,
        top,
        side,
        tabs,
        body,
        footer,
        terminal,
    }
}

impl Geometry {
    pub fn inside_modal(self, modal: &Modal, (x, y): (u16, u16)) -> bool {
        modal_area(self.area, modal).contains(Position::new(x, y))
    }

    pub fn size(self) -> TerminalSize {
        TerminalSize {
            rows: self.terminal.height.max(1),
            cols: self.terminal.width.max(2),
        }
    }

    fn side_inner(self) -> Rect {
        Block::bordered().inner(self.side)
    }

    pub fn control_rows(self) -> usize {
        self.side_inner().height as usize
    }

    pub fn control_at(self, (x, y): (u16, u16), offset: usize) -> Option<usize> {
        let inner = self.side_inner();
        inner
            .contains(Position::new(x, y))
            .then(|| usize::from(y - inner.y) + offset)
    }

    pub fn inside_side(self, (x, y): (u16, u16)) -> bool {
        self.side.contains(Position::new(x, y))
    }

    pub fn inside_body(self, (x, y): (u16, u16)) -> bool {
        self.terminal.contains(Position::new(x, y))
    }

    pub fn cell_at(self, (x, y): (u16, u16)) -> Option<(u16, u16)> {
        self.inside_body((x, y))
            .then(|| (y - self.terminal.y, x - self.terminal.x))
    }

    pub fn file_rows(self) -> usize {
        self.terminal.height.saturating_sub(3).into()
    }

    pub fn file_at(self, position: (u16, u16), scroll: usize) -> Option<usize> {
        let (x, y) = position;
        let (_, list) = self.file_columns(self.terminal);
        if y < list.y.saturating_add(3) || !list.contains(Position::new(x, y)) {
            return None;
        }
        Some(usize::from(y - list.y - 3) + scroll)
    }

    fn file_columns(self, area: Rect) -> (Rect, Rect) {
        if self.terminal.width < 72 {
            return (Rect::ZERO, area);
        }
        let [tree, list] =
            Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).areas(area);
        (tree, list)
    }

    pub fn tree_at(self, (x, y): (u16, u16)) -> Option<(usize, usize)> {
        let (tree, _) = self.file_columns(self.terminal);
        if !tree.contains(Position::new(x, y)) || y < tree.y.saturating_add(3) {
            return None;
        }
        Some((usize::from(y - tree.y - 3), usize::from(x - tree.x)))
    }

    pub fn in_file_actions(self, (x, y): (u16, u16)) -> bool {
        self.inside_body((x, y)) && y == self.terminal.y
    }
}

pub fn controls(app: &App) -> Vec<(String, Action)> {
    let serial = &app.settings.serial;
    let selected = app.selected_device();
    let device = &app.settings.adb_device;
    let has_shell = app
        .shells
        .get(device)
        .is_some_and(|session| session.opening || session.connection.is_some());
    let serial_connected = app.serial.connection.is_some();
    let status = selected
        .map(|device| device_state(&device.state))
        .unwrap_or("未连接");
    let remote_connected = app
        .devices
        .iter()
        .any(|device| device.online() && device.id == app.settings.adb_address);
    let parity = match serial.parity.as_str() {
        "even" => "偶",
        "odd" => "奇",
        _ => "无",
    };
    let show = |value: &str| {
        if value.is_empty() {
            "—".into()
        } else {
            value.to_owned()
        }
    };
    vec![
        (
            format!("串口  {}  ▾", show(&serial.port_name)),
            Action::Port,
        ),
        (format!("波特率  {}  ▾", serial.baud_rate), Action::Baud),
        (format!("数据位  {}  ▾", serial.data_bits), Action::DataBits),
        (format!("校验位  {parity}  ▾"), Action::Parity),
        (format!("停止位  {}  ▾", serial.stop_bits), Action::StopBits),
        ("刷新串口".into(), Action::Ports),
        (
            if app.serial.opening {
                "取消打开串口"
            } else if serial_connected {
                "● 关闭串口"
            } else {
                "○ 打开串口"
            }
            .into(),
            Action::Serial,
        ),
        (
            format!(
                "自动尺寸同步  {}",
                if app.settings.serial_sync {
                    "开"
                } else {
                    "关"
                }
            ),
            Action::SerialSync,
        ),
        (format!("ADB  {}  ▾", show(device)), Action::Device),
        (format!("刷新 ADB  ·  {status}"), Action::Devices),
        (
            format!("地址  {}", app.settings.adb_address),
            Action::Address,
        ),
        (
            if remote_connected {
                "断开远程 ADB"
            } else {
                "连接远程 ADB"
            }
            .into(),
            Action::Remote,
        ),
        (
            format!("视频编码  {}  ▾", app.settings.video_codec),
            Action::Codec,
        ),
        (
            format!("视频码率  {}", app.settings.video_bit_rate),
            Action::Bitrate,
        ),
        (
            if app.scrcpy.contains_key(device) {
                "● 关闭 scrcpy"
            } else {
                "○ 打开 scrcpy"
            }
            .into(),
            Action::Scrcpy,
        ),
        (
            if has_shell {
                "● 关闭 ADB Shell"
            } else {
                "○ 打开 ADB Shell"
            }
            .into(),
            Action::Shell,
        ),
        ("浏览设备文件".into(), Action::Files),
        ("ADB ROOT".into(), Action::Root),
        ("ADB REMOUNT".into(), Action::Remount),
        ("ADB SYNC".into(), Action::Sync),
    ]
}

fn label(app: &App, tab: &Tab) -> String {
    match tab {
        Tab::Serial => format!(
            " {} 串口终端 ",
            if app.serial.connection.is_some() {
                "●"
            } else {
                "○"
            }
        ),
        Tab::Shell(id) => format!(
            " {} Shell · {id} ",
            if app
                .shells
                .get(id)
                .is_some_and(|session| session.connection.is_some())
            {
                "●"
            } else {
                "○"
            }
        ),
        Tab::Files(id) => format!(" 文件 · {id} "),
    }
}

fn visible_tabs(app: &App, geometry: Geometry) -> Vec<(usize, String, u16, u16)> {
    let mut start = 0;
    let mut lengths = Vec::with_capacity(app.tabs.len());
    for tab in &app.tabs {
        let label = label(app, tab);
        let width = Line::raw(&label).width().min(u16::MAX.into()) as u16;
        lengths.push((label, width));
    }
    let mut width = 0;
    for (index, (_, len)) in lengths.iter().enumerate().take(app.active + 1) {
        width += u32::from(*len);
        while width > u32::from(geometry.tabs.width) && start < index {
            width -= u32::from(lengths[start].1);
            start += 1;
        }
    }
    let mut left = geometry.tabs.x;
    let mut visible = Vec::new();
    for (index, (label, len)) in lengths.into_iter().enumerate().skip(start) {
        let right = left.saturating_add(len).min(geometry.tabs.right());
        if left >= right {
            break;
        }
        visible.push((index, label, left, right));
        left = right;
    }
    visible
}

pub fn tab_at(app: &App, geometry: Geometry, (x, y): (u16, u16)) -> Option<usize> {
    if y != geometry.tabs.y {
        return None;
    }
    visible_tabs(app, geometry)
        .into_iter()
        .find_map(|(index, _, left, right)| (left <= x && x < right).then_some(index))
}

fn base() -> Style {
    Style::default().fg(TEXT).bg(BG)
}
fn faint() -> Style {
    Style::default().fg(MUTED)
}
fn accent() -> Style {
    Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
}

fn panel(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused { BLUE } else { HIGHLIGHT }))
}

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let g = geometry(frame.area());
    frame.render_widget(Block::default().style(base()), g.area);
    let top = Line::from(vec![
        Span::styled(" Serial Terminal ", accent()),
        Span::styled("· Ratatui", faint()),
        Span::styled("   Ctrl+F1 帮助", faint()),
    ]);
    frame.render_widget(
        Paragraph::new(top).style(Style::default().bg(SURFACE)),
        g.top,
    );
    if g.area.width < 24 || g.area.height < 7 {
        frame.render_widget(Paragraph::new("请放大终端窗口").centered(), g.body);
        return;
    }

    let side = panel(" 功能区 ", app.sidebar).style(Style::default().bg(SIDE));
    let lines = controls(app)
        .into_iter()
        .enumerate()
        .skip(app.control_scroll)
        .take(g.control_rows())
        .map(|(index, (label, action))| {
            let style = if app.sidebar && index == app.control {
                Style::default()
                    .bg(HIGHLIGHT)
                    .fg(BLUE)
                    .add_modifier(Modifier::BOLD)
            } else if matches!(
                action,
                Action::Serial | Action::Scrcpy | Action::Shell | Action::Files
            ) {
                Style::default().fg(GREEN)
            } else {
                base().bg(SIDE)
            };
            Line::styled(
                format!(
                    " {}{label}",
                    if app.sidebar && index == app.control {
                        "› "
                    } else {
                        "  "
                    }
                ),
                style,
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(side, g.side);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(SIDE)),
        g.side_inner(),
    );

    frame.render_widget(Block::default().style(Style::default().bg(SIDE)), g.tabs);
    for (index, label, left, right) in visible_tabs(app, g) {
        let style = if index == app.active {
            accent().bg(SURFACE)
        } else {
            faint().bg(SIDE)
        };
        frame.render_widget(
            Paragraph::new(label).style(style),
            Rect::new(left, g.tabs.y, right - left, 1),
        );
    }

    match app.active_tab() {
        Tab::Files(device) => render_files(frame, app, g, device),
        _ => render_session(frame, app, g),
    }

    let tone = if app.error { RED } else { MUTED };
    frame.render_widget(
        Paragraph::new(format!(
            " {}   ·   Ctrl+F2 功能区  Ctrl+F3 内容  Ctrl+F10 退出",
            app.status
        ))
        .style(Style::default().bg(SURFACE).fg(tone)),
        g.footer,
    );
    if let Some(modal) = &app.modal {
        render_modal(frame, g.area, modal);
    }
}

fn render_session(frame: &mut Frame<'_>, app: &App, g: Geometry) {
    let Some(session) = app.session() else {
        return;
    };
    let title = if app.active_tab() == &Tab::Serial {
        let status = if session.opening {
            "连接中"
        } else if session.connection.is_some() {
            "已连接"
        } else {
            "未连接"
        };
        format!(" {status} · RX {} B · TX {} B ", session.rx, session.tx)
    } else if session.connection.is_some() {
        " ADB Shell · 已连接 ".into()
    } else if session.opening {
        " ADB Shell · 正在连接 ".into()
    } else {
        " ADB Shell · 已退出 ".into()
    };
    frame.render_widget(panel(title, !app.sidebar), g.body);
    let show_cursor = !app.sidebar
        && app.modal.is_none()
        && session.connection.is_some()
        && session.screen().scrollback() == 0;
    let cursor = Cursor::default()
        .visibility(show_cursor)
        .symbol("▏")
        .style(base().fg(TERMINAL_CURSOR))
        .overlay_style(Style::reset().fg(BG).bg(TERMINAL_CURSOR));
    frame.render_widget(
        PseudoTerminal::new(session.screen()).cursor(cursor),
        g.terminal,
    );
    // tui-term maps VT defaults to Reset and the 16 ANSI colours to the host
    // palette. Resolve those cells to the app theme before flushing the frame.
    let buffer = frame.buffer_mut();
    for row in g.terminal.rows() {
        for position in row.positions() {
            let cell = &mut buffer[position];
            cell.fg = terminal_color(cell.fg, TEXT);
            cell.bg = terminal_color(cell.bg, BG);
        }
    }
    if let Some((start, end)) = session.selection {
        let (start, end) = if start > end {
            (end, start)
        } else {
            (start, end)
        };
        for row in start.0..=end.0.min(g.terminal.height.saturating_sub(1)) {
            let from = if row == start.0 { start.1 } else { 0 };
            let to = if row == end.0 {
                end.1
            } else {
                g.terminal.width.saturating_sub(1)
            };
            for col in from..=to.min(g.terminal.width.saturating_sub(1)) {
                if let Some(cell) = frame
                    .buffer_mut()
                    .cell_mut((g.terminal.x + col, g.terminal.y + row))
                {
                    cell.set_bg(TERMINAL_SELECTION);
                }
            }
        }
    }
}

fn terminal_color(color: Color, default: Color) -> Color {
    match color {
        Color::Reset => default,
        Color::Indexed(index @ 0..=15) => TERMINAL_ANSI[usize::from(index)],
        color => color,
    }
}

fn render_files(frame: &mut Frame<'_>, app: &App, g: Geometry, device: &str) {
    let Some(browser) = app.files.get(device) else {
        return;
    };
    let status = if browser.loading {
        " · 正在读取"
    } else if browser.busy {
        " · 处理中"
    } else {
        ""
    };
    frame.render_widget(
        panel(format!(" 设备文件 · {device}{status} "), !app.sidebar),
        g.body,
    );
    let [actions, path, content] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(g.terminal);
    frame.render_widget(
        Paragraph::new(" 操作 ▾  R 刷新  U 上传  D 下载  N 新建  F2 重命名  Del 删除  S 刷盘")
            .style(accent().bg(SURFACE)),
        actions,
    );
    frame.render_widget(
        Paragraph::new(format!(
            " {}   ·   {} 项  已选 {}",
            browser.path,
            browser.entries.len(),
            browser.marked.len()
        ))
        .style(faint()),
        path,
    );
    let (tree, files) = g.file_columns(content);
    if tree.width > 0 {
        let mut tree_lines = vec![Line::styled(" 目录树", faint().bg(SIDE))];
        tree_lines.extend(
            browser
                .tree()
                .into_iter()
                .skip(browser.tree_scroll)
                .take(g.file_rows())
                .map(|(depth, directory, name)| {
                    let marker = if browser.expanded.contains(directory) {
                        '▾'
                    } else {
                        '▸'
                    };
                    let style = if directory == browser.path {
                        accent().bg(HIGHLIGHT)
                    } else {
                        faint()
                    };
                    Line::styled(
                        format!("{}{marker} {name}", " ".repeat(depth.saturating_mul(2))),
                        style,
                    )
                }),
        );
        frame.render_widget(
            Paragraph::new(tree_lines).block(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(HIGHLIGHT)),
            ),
            tree,
        );
    }
    let [header, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(files);
    file_row(
        frame,
        header,
        "  名称",
        "大小",
        "修改于",
        "权限",
        faint().bg(SIDE),
    );
    for (row, entry) in browser
        .entries
        .iter()
        .enumerate()
        .skip(browser.scroll)
        .take(usize::from(list.height))
    {
        let area = Rect::new(
            list.x,
            list.y + (row - browser.scroll) as u16,
            list.width,
            1,
        );
        let checked = browser.marked.contains(&entry.path);
        let style = if row == browser.cursor {
            Style::default().fg(BLUE).bg(HIGHLIGHT)
        } else if entry.directory {
            Style::default().fg(BLUE)
        } else {
            base()
        };
        let marker = if checked { "◆" } else { " " };
        let kind = if entry.directory { "▸" } else { " " };
        let name = format!("{marker}{kind} {}", entry.name);
        file_row(
            frame,
            area,
            &name,
            &file_size(entry),
            &modified(entry.modified),
            &entry.mode,
            style,
        );
    }
    if browser.entries.is_empty() && !browser.loading {
        frame.render_widget(Paragraph::new("  目录为空").style(faint()), list);
    }
}

fn file_row(
    frame: &mut Frame<'_>,
    area: Rect,
    name: &str,
    size: &str,
    time: &str,
    mode: &str,
    style: Style,
) {
    frame.render_widget(Block::default().style(style), area);
    if area.width < 48 {
        frame.render_widget(Paragraph::new(name).style(style), area);
        return;
    }
    let [name_area, size_area, time_area, mode_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(6),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(name).style(style), name_area);
    frame.render_widget(Paragraph::new(size).style(style), size_area);
    frame.render_widget(Paragraph::new(time).style(style), time_area);
    frame.render_widget(Paragraph::new(mode).style(style), mode_area);
}

fn file_size(entry: &Entry) -> String {
    if entry.directory {
        return "—".into();
    }
    if entry.size < 1024 {
        return format!("{} B", entry.size);
    }
    let mut size = entry.size as f64;
    for unit in ["KiB", "MiB", "GiB"] {
        size /= 1024.0;
        if size < 1024.0 || unit == "GiB" {
            return format!("{size:.1} {unit}");
        }
    }
    String::new()
}

fn modified(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let elapsed = now.saturating_sub(timestamp as u64);
    match elapsed {
        0..60 => "刚刚".into(),
        60..3600 => format!("{} 分钟前", elapsed / 60),
        3600..86400 => format!("{} 小时前", elapsed / 3600),
        _ => format!("{} 天前", elapsed / 86400),
    }
}

fn modal_area(area: Rect, modal: &Modal) -> Rect {
    let desired_height = match modal {
        Modal::Pick { options, .. } => options.len().min(16) as u16 + 3,
        Modal::Edit { .. } => 7,
        Modal::Upload(_) => 8,
        Modal::Help => 13,
    };
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(2).min(desired_height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub fn modal_option_at(geometry: Geometry, modal: &Modal, (x, y): (u16, u16)) -> Option<usize> {
    let Modal::Pick { options, index, .. } = modal else {
        return None;
    };
    let area = Block::bordered().inner(modal_area(geometry.area, modal));
    if !area.contains(Position::new(x, y)) {
        return None;
    }
    let rows = usize::from(area.height.saturating_sub(1).max(1));
    let start = index.saturating_sub(rows - 1);
    let item = start + usize::from(y - area.y);
    (item < options.len() && usize::from(y - area.y) < rows).then_some(item)
}

fn render_modal(frame: &mut Frame<'_>, area: Rect, modal: &Modal) {
    let area = modal_area(area, modal);
    let title = match modal {
        Modal::Pick { title, .. } | Modal::Edit { title, .. } => title.clone(),
        Modal::Upload(upload) => format!("上传重命名 {}/{}", upload.index + 1, upload.files.len()),
        Modal::Help => "操作说明".into(),
    };
    frame.render_widget(Clear, area);
    let block = panel(format!(" {title} "), true).style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match modal {
        Modal::Pick { options, index, .. } => {
            let rows = usize::from(inner.height.saturating_sub(1).max(1));
            let start = index.saturating_sub(rows - 1);
            let mut lines = options
                .iter()
                .enumerate()
                .skip(start)
                .take(rows)
                .map(|(item, (label, _))| {
                    Line::styled(
                        format!(" {} {label}", if index == &item { "›" } else { " " }),
                        if index == &item {
                            accent().bg(HIGHLIGHT)
                        } else {
                            base().bg(SURFACE)
                        },
                    )
                })
                .collect::<Vec<_>>();
            if options.is_empty() {
                lines.push(Line::styled("  当前无可选项目", faint()));
            }
            lines.push(Line::styled(" ↑↓ 选择  Enter 确认  Esc 取消", faint()));
            frame.render_widget(Paragraph::new(lines), inner);
        }
        Modal::Edit { value, .. } => render_edit(frame, inner, value, ""),
        Modal::Upload(upload) => {
            let (path, value) = &upload.files[upload.index];
            render_edit(frame, inner, value, &path.display().to_string());
        }
        Modal::Help => {
            let lines = Text::from(vec![
                Line::raw(" Ctrl+F2 / Ctrl+F3    功能区 / 当前内容"),
                Line::raw(" Ctrl+F4 / Ctrl+F5    上一个 / 下一个标签"),
                Line::raw(" Ctrl+F6 / Ctrl+F10   关闭标签 / 退出"),
                Line::raw(" 终端  拖选、右键、滚轮、Shift+PageUp/Down"),
                Line::raw(" 文件  Enter 打开、退格返回、Alt+←/→ 历史"),
                Line::raw(" 文件  空格或 Ctrl+单击多选、右键打开操作"),
                Line::raw(" 文件  Ctrl+C / Ctrl+V  复制 / 粘贴文件"),
                Line::styled(" Esc 关闭帮助", faint()),
            ]);
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }
}

fn render_edit(frame: &mut Frame<'_>, area: Rect, value: &str, note: &str) {
    let lines = vec![
        Line::styled(note.to_owned(), faint()),
        Line::styled(format!(" {value}▏"), accent()),
        Line::raw(""),
        Line::styled(" Enter 确认  Ctrl+U 清空  Esc 取消", faint()),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod terminal_colors {
    use super::*;
    use crate::terminal::Session;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn terminal_renders_theme_defaults_and_ansi_without_losing_extended_colors() {
        let mut app = App::new();
        let size = Rect::new(0, 0, 100, 28);
        let geometry = geometry(size);
        app.serial = Session::new(0, geometry.size());
        app.serial
            .output(b"D\x1b[31;44mA\x1b[38;5;123;48;2;10;20;30mB\x1b[0mC");
        app.serial.selection = Some(((0, 3), (0, 3)));
        let mut display = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        display.draw(|frame| render(frame, &app)).unwrap();
        let buffer = display.backend().buffer();
        let terminal = geometry.terminal;
        let cell = |index| &buffer[(terminal.x + index, terminal.y)];
        assert_eq!((cell(0).fg, cell(0).bg), (TEXT, BG));
        assert_eq!((cell(1).fg, cell(1).bg), (RED, BLUE));
        assert_eq!(
            (cell(2).fg, cell(2).bg),
            (Color::Indexed(123), Color::Rgb(10, 20, 30))
        );
        assert_eq!((cell(3).fg, cell(3).bg), (TEXT, TERMINAL_SELECTION));
        assert_eq!((cell(4).fg, cell(4).bg), (TEXT, BG));
    }
}
