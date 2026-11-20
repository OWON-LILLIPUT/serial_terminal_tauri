use crate::{adb_shell_protocol::TerminalSize, backend::Connection};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vt100::{Callbacks, Parser, Screen};

#[derive(Default)]
struct Replies(Vec<u8>);

impl Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        action: char,
    ) {
        if i1.is_some() || i2.is_some() {
            return;
        }
        let bytes = match (action, params.first().and_then(|p| p.first()).copied()) {
            ('t', Some(18)) => {
                let (rows, cols) = screen.size();
                format!("\x1b[8;{rows};{cols}t")
            }
            ('n', Some(6)) => {
                let (row, col) = screen.cursor_position();
                format!("\x1b[{};{}R", row + 1, col + 1)
            }
            ('n', Some(5)) => "\x1b[0n".into(),
            ('c', None | Some(0)) => "\x1b[?1;2c".into(),
            _ => return,
        };
        self.0.extend(bytes.bytes());
    }
}

struct ResizeEdge {
    row: u16,
    cell: String,
    returning: bool,
    last_byte: u8,
}

pub struct Session {
    parser: Parser<Replies>,
    pub connection: Option<Connection>,
    pub id: u64,
    pub opening: bool,
    pub size: TerminalSize,
    pub rx: u64,
    pub tx: u64,
    pub selection: Option<((u16, u16), (u16, u16))>,
    edge: Option<ResizeEdge>,
}

impl Session {
    pub fn new(id: u64, size: TerminalSize) -> Self {
        Self {
            parser: Parser::new_with_callbacks(size.rows, size.cols, 10000, Replies::default()),
            connection: None,
            id,
            opening: false,
            size,
            rx: 0,
            tx: 0,
            selection: None,
            edge: None,
        }
    }

    pub fn screen(&self) -> &Screen {
        self.parser.screen()
    }

    pub fn resize(&mut self, size: TerminalSize, serial_cleanup: bool) {
        if self.size == size {
            return;
        }
        let shrank = size.cols < self.size.cols;
        self.size = size;
        self.selection = None;
        self.parser.screen_mut().set_size(size.rows, size.cols);
        self.edge = None;
        if shrank
            && serial_cleanup
            && !self.screen().alternate_screen()
            && self.screen().scrollback() == 0
        {
            let (row, _) = self.screen().cursor_position();
            if let Some(cell) = self
                .screen()
                .cell(row, size.cols - 1)
                .filter(|cell| cell.has_contents() && !cell.contents().trim().is_empty())
            {
                self.edge = Some(ResizeEdge {
                    row,
                    cell: cell.contents().into(),
                    returning: false,
                    last_byte: 0,
                });
            }
        }
    }

    pub fn output(&mut self, bytes: &[u8]) {
        self.rx += bytes.len() as u64;
        let mut backspace = None;
        if let Some(edge) = &mut self.edge {
            for (index, &byte) in bytes.iter().enumerate() {
                if byte == b'\n' {
                    self.edge = None;
                    break;
                }
                if byte == b'\r' {
                    edge.returning = true;
                } else if byte < b' ' && edge.returning {
                    edge.returning = false;
                    if byte == 8 && matches!(edge.last_byte, b' ' | b'<' | b'>' | b'*') {
                        backspace = Some(index);
                        break;
                    }
                }
                edge.last_byte = byte;
            }
        }
        if let Some(index) = backspace {
            self.parser.process(&bytes[..index]);
            let edge = self.edge.take().unwrap();
            let last = self.size.cols - 1;
            if self.screen().cursor_position() == (edge.row, last)
                && !self.screen().alternate_screen()
                && self.screen().scrollback() == 0
                && self
                    .screen()
                    .cell(edge.row, last)
                    .is_some_and(|cell| cell.contents() == edge.cell)
            {
                // mksh redraw ends at the previous column before its backspaces.
                self.parser.process(b"\x1b[X");
            }
            self.parser.process(&bytes[index..]);
        } else {
            self.parser.process(bytes);
        }
        self.flush_replies();
    }

    pub fn flush_replies(&mut self) {
        if self.connection.is_some() && !self.parser.callbacks().0.is_empty() {
            let replies = std::mem::take(&mut self.parser.callbacks_mut().0);
            self.send(replies);
        }
    }

    pub fn send(&mut self, bytes: Vec<u8>) {
        if let Some(connection) = &self.connection {
            self.tx += bytes.len() as u64;
            connection.write(bytes, self.size);
        }
    }

    pub fn paste(&mut self, text: String) {
        let data = if self.screen().bracketed_paste() {
            format!("\x1b[200~{text}\x1b[201~")
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        self.send(data.into_bytes());
    }

    pub fn scroll(&mut self, offset: isize) {
        self.selection = None;
        let next = self.screen().scrollback().saturating_add_signed(offset);
        self.parser.screen_mut().set_scrollback(next);
    }

    pub fn clear(&mut self) {
        self.selection = None;
        self.edge = None;
        self.parser.process(b"\x1b[2J\x1b[3J\x1b[H");
    }

    pub fn selected_text(&self) -> Option<String> {
        let (first, last) = self.selection?;
        let ((row, col), (end_row, end_col)) = if first > last {
            (last, first)
        } else {
            (first, last)
        };
        Some(
            self.screen()
                .contents_between(row, col, end_row, end_col.saturating_add(1)),
        )
    }
}

pub fn encode_key(event: KeyEvent, screen: &Screen) -> Vec<u8> {
    let modifiers = event.modifiers;
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let special = |letter: char| {
        let modifier = 1
            + u8::from(modifiers.contains(KeyModifiers::SHIFT))
            + 2 * u8::from(alt)
            + 4 * u8::from(ctrl);
        if modifier > 1 {
            format!("\x1b[1;{modifier}{letter}")
        } else if screen.application_cursor() {
            format!("\x1bO{letter}")
        } else {
            format!("\x1b[{letter}")
        }
    };
    match event.code {
        KeyCode::Char(ch) => {
            let mut bytes = Vec::with_capacity(5);
            if alt && !ctrl {
                bytes.push(27);
            }
            if ctrl && !alt && (ch.is_ascii_alphabetic() || "@[\\]^_ ?".contains(ch)) {
                bytes.push(if ch == '?' {
                    127
                } else {
                    ch.to_ascii_uppercase() as u8 & 31
                });
            } else {
                bytes.extend(ch.encode_utf8(&mut [0; 4]).bytes());
            }
            bytes
        }
        KeyCode::Enter => vec![13],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![9],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![27],
        KeyCode::Up => special('A').into_bytes(),
        KeyCode::Down => special('B').into_bytes(),
        KeyCode::Right => special('C').into_bytes(),
        KeyCode::Left => special('D').into_bytes(),
        KeyCode::Home => special('H').into_bytes(),
        KeyCode::End => special('F').into_bytes(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n @ 1..=4) => vec![27, b'O', b'P' + n - 1],
        KeyCode::F(n @ 5..=12) => {
            let number = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)];
            format!("\x1b[{number}~").into_bytes()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_redraw_after_shrink_erases_reserved_cell_and_keeps_position() {
        let mut session = Session::new(1, TerminalSize { rows: 5, cols: 24 });
        session.output(b"abcdefghijklmnopqrst");
        session.resize(TerminalSize { rows: 5, cols: 12 }, true);
        session.output(b"\r");
        session.output(b"hello     <");
        session.output(b"\x08\x08\x08");
        assert_eq!(session.screen().cursor_position(), (0, 8));
        assert_eq!(session.screen().cell(0, 11).unwrap().contents(), "");
        assert_eq!(session.screen().cell(0, 10).unwrap().contents(), "<");
    }

    #[test]
    fn full_width_output_keeps_a_character_written_at_the_edge() {
        let mut session = Session::new(1, TerminalSize { rows: 5, cols: 24 });
        session.output(b"<<<<<<<<<<<<<<<<<<<<");
        session.resize(TerminalSize { rows: 5, cols: 12 }, true);
        session.output(b"\rfull width <\x08");
        assert_eq!(session.screen().cell(0, 11).unwrap().contents(), "<");
    }

    #[test]
    fn reports_exact_inner_size_and_encodes_tab_and_unicode() {
        let mut session = Session::new(1, TerminalSize { rows: 12, cols: 23 });
        session.output(b"\x1b[18t");
        assert_eq!(session.parser.callbacks().0, b"\x1b[8;12;23t");
        assert_eq!(
            encode_key(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                session.screen()
            ),
            b"\t"
        );
        assert_eq!(
            encode_key(
                KeyEvent::new(KeyCode::Char('中'), KeyModifiers::NONE),
                session.screen()
            ),
            "中".as_bytes()
        );
    }
}
