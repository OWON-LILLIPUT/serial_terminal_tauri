//! Interactive PTY over the existing ADB server's smart socket.
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const STDIN: u8 = 0;
const STDOUT: u8 = 1;
const STDERR: u8 = 2;
const EXIT: u8 = 3;
const WINDOW_SIZE: u8 = 5;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

pub fn open(endpoint: &str, device: &str, size: TerminalSize) -> io::Result<Option<TcpStream>> {
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "ADB server address not found");
    for addr in endpoint.to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, IO_TIMEOUT) {
            Ok(mut stream) => {
                stream.set_nodelay(true)?;
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                if let Some(error) = request(&mut stream, &format!("host:transport:{device}"))? {
                    return Err(io::Error::other(error));
                }
                if request(&mut stream, "shell,v2,TERM=xterm-256color,pty:")?.is_some() {
                    // The CLI can fall back to the legacy protocol on older devices.
                    return Ok(None);
                }
                send_size(&mut stream, size)?;
                stream.set_read_timeout(None)?;
                return Ok(Some(stream));
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn request(stream: &mut TcpStream, service: &str) -> io::Result<Option<String>> {
    let len = u16::try_from(service.len()).map_err(|_| invalid("ADB service name too long"))?;
    write!(stream, "{len:04x}{service}")?;
    let mut status = [0; 4];
    stream.read_exact(&mut status)?;
    match &status {
        b"OKAY" => Ok(None),
        b"FAIL" => {
            stream.read_exact(&mut status)?;
            let length = std::str::from_utf8(&status)
                .ok()
                .and_then(|len| u16::from_str_radix(len, 16).ok())
                .ok_or_else(|| invalid("invalid ADB error length"))?;
            let mut message = vec![0; length.into()];
            stream.read_exact(&mut message)?;
            Ok(Some(String::from_utf8_lossy(&message).into_owned()))
        }
        _ => Err(invalid("invalid ADB server response")),
    }
}

fn send_size(writer: &mut impl Write, size: TerminalSize) -> io::Result<()> {
    let payload = format!("{}x{},0x0\0", size.rows, size.cols);
    send_packet(writer, WINDOW_SIZE, payload.as_bytes())
}

fn send_input(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    for chunk in bytes.chunks(16 * 1024) {
        send_packet(writer, STDIN, chunk)?;
    }
    Ok(())
}

pub fn update_and_write(
    writer: &mut impl Write,
    current_size: &mut TerminalSize,
    next_size: TerminalSize,
    bytes: &[u8],
) -> io::Result<()> {
    if *current_size != next_size {
        send_size(writer, next_size)?;
        *current_size = next_size;
    }
    send_input(writer, bytes)
}

fn send_packet(writer: &mut impl Write, id: u8, bytes: &[u8]) -> io::Result<()> {
    let mut packet = Vec::with_capacity(5 + bytes.len());
    packet.push(id);
    packet.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    packet.extend_from_slice(bytes);
    writer.write_all(&packet)
}

pub fn read_output(reader: &mut impl Read, mut output: impl FnMut(&[u8])) -> io::Result<u8> {
    let mut buffer = [0_u8; 4096];
    loop {
        let mut header = [0; 5];
        reader.read_exact(&mut header)?;
        let mut left = u32::from_le_bytes(header[1..].try_into().unwrap());
        match (header[0], left) {
            (EXIT, 1) => {
                reader.read_exact(&mut buffer[..1])?;
                return Ok(buffer[0]);
            }
            (STDOUT | STDERR, _) => {
                while left > 0 {
                    let count = (left as usize).min(buffer.len());
                    reader.read_exact(&mut buffer[..count])?;
                    output(&buffer[..count]);
                    left -= count as u32;
                }
            }
            _ => return Err(invalid("unexpected ADB shell packet")),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn expect_request(stream: &mut TcpStream, expected: &str) {
        let mut bytes = vec![0; 4 + expected.len()];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            format!("{:04x}{expected}", expected.len()).as_bytes()
        );
        stream.write_all(b"OKAY").unwrap();
    }

    fn receive_packet(stream: &mut TcpStream, id: u8, expected: &[u8]) {
        let mut bytes = vec![0; 5 + expected.len()];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes[0], id);
        assert_eq!(bytes[1..5], (expected.len() as u32).to_le_bytes());
        assert_eq!(&bytes[5..], expected);
    }

    #[test]
    fn sends_initial_size_and_shrinks_pty_before_tab_input() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
            expect_request(&mut stream, "host:transport:TEST:5555");
            expect_request(&mut stream, "shell,v2,TERM=xterm-256color,pty:");
            receive_packet(&mut stream, 5, b"24x120,0x0\0");
            receive_packet(&mut stream, 5, b"24x32,0x0\0");
            receive_packet(&mut stream, 0, b"\t");
            receive_packet(&mut stream, 0, b"\x1b[D");
            send_packet(&mut stream, 1, b"\r\x1b[K$ completed").unwrap();
            send_packet(&mut stream, 2, b"!").unwrap();
            send_packet(&mut stream, 3, &[0]).unwrap();
        });
        let mut size = TerminalSize {
            rows: 24,
            cols: 120,
        };
        let mut shell = open(&endpoint, "TEST:5555", size).unwrap().unwrap();
        let narrow = TerminalSize { rows: 24, cols: 32 };
        update_and_write(&mut shell, &mut size, narrow, b"\t").unwrap();
        update_and_write(&mut shell, &mut size, narrow, b"\x1b[D").unwrap();
        let mut output = Vec::new();
        assert_eq!(
            read_output(&mut shell, |bytes| output.extend_from_slice(bytes)).unwrap(),
            0
        );
        assert_eq!(output, b"\r\x1b[K$ completed!");
        peer.join().unwrap();
    }

    #[test]
    fn rejects_invalid_shell_output() {
        let mut input: &[u8] = &[EXIT, 2, 0, 0, 0, 0, 0];
        assert_eq!(
            read_output(&mut input, |_| unreachable!())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn unsupported_shell_protocol_leaves_legacy_fallback_available() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
            expect_request(&mut stream, "host:transport:LEGACY");
            let expected = b"shell,v2,TERM=xterm-256color,pty:";
            let mut service = vec![0; 4 + expected.len()];
            stream.read_exact(&mut service).unwrap();
            assert_eq!(&service[4..], expected);
            stream.write_all(b"FAIL0007unknown").unwrap();
        });
        let size = TerminalSize { rows: 24, cols: 32 };
        assert!(open(&endpoint, "LEGACY", size).unwrap().is_none());
        peer.join().unwrap();
    }
}
