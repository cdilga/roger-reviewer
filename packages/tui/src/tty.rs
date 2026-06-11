//! Minimal Roger-owned raw-mode TTY event source.
//!
//! Why this exists: ftui 0.4.0's native terminal backend (`ftui-tty`) was
//! never published at 0.4.0, and the crossterm compat backend is excluded by
//! Roger dependency policy. ftui's `Program::with_event_source` is the
//! documented extension point for exactly this situation, so this module
//! supplies the smallest honest Unix backend: termios raw mode via `libc`,
//! `poll(2)` for input readiness, `TIOCGWINSZ` for size, and ftui-core's own
//! `InputParser` for canonical event decoding. Rendering, diffing, and the
//! runtime loop all stay FrankenTUI's.

use ftui::Event;
use ftui::core::input_parser::InputParser;
use ftui::runtime::{BackendEventSource, BackendFeatures};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::Duration;

const STDIN_FD: libc::c_int = libc::STDIN_FILENO;

const ALT_SCREEN_ENTER: &[u8] = b"\x1b[?1049h";
const ALT_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";
const CURSOR_SHOW: &[u8] = b"\x1b[?25h";
const CURSOR_HIDE: &[u8] = b"\x1b[?25l";
const SGR_RESET: &[u8] = b"\x1b[0m";

/// RAII guard for raw mode + alternate screen. Restores the original termios
/// state and leaves the alternate screen on drop (including panic unwinds).
struct RawModeGuard {
    original: libc::termios,
}

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        // SAFETY: plain libc termios calls on the stdin fd with a valid
        // zero-initialized struct; error returns are checked.
        unsafe {
            let mut original: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(STDIN_FD, &mut original) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut raw = original;
            libc::cfmakeraw(&mut raw);
            // Non-blocking-ish reads: poll(2) decides readiness, read drains
            // whatever is available without waiting for more bytes.
            raw.c_cc[libc::VMIN] = 0;
            raw.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(STDIN_FD, libc::TCSAFLUSH, &raw) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut stdout = io::stdout();
            let _ = stdout.write_all(ALT_SCREEN_ENTER);
            let _ = stdout.write_all(CURSOR_HIDE);
            let _ = stdout.flush();
            Ok(Self { original })
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(SGR_RESET);
        let _ = stdout.write_all(CURSOR_SHOW);
        let _ = stdout.write_all(ALT_SCREEN_LEAVE);
        let _ = stdout.flush();
        // SAFETY: restoring the termios snapshot captured in `enter`.
        unsafe {
            let _ = libc::tcsetattr(STDIN_FD, libc::TCSAFLUSH, &self.original);
        }
    }
}

pub(crate) struct RogerTtyEventSource {
    _raw_guard: RawModeGuard,
    parser: InputParser,
    pending: VecDeque<Event>,
    last_size: (u16, u16),
}

impl RogerTtyEventSource {
    pub(crate) fn open() -> io::Result<Self> {
        let raw_guard = RawModeGuard::enter()?;
        let last_size = terminal_size()?;
        Ok(Self {
            _raw_guard: raw_guard,
            parser: InputParser::new(),
            pending: VecDeque::new(),
            last_size,
        })
    }

    fn drain_stdin(&mut self) -> io::Result<()> {
        let mut buf = [0u8; 1024];
        loop {
            // SAFETY: reading into a stack buffer of the stated length.
            let read =
                unsafe { libc::read(STDIN_FD, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
            match read {
                0 => return Ok(()),
                n if n > 0 => {
                    for event in self.parser.parse(&buf[..n as usize]) {
                        self.pending.push_back(event);
                    }
                    if (n as usize) < buf.len() {
                        return Ok(());
                    }
                }
                _ => {
                    let err = io::Error::last_os_error();
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) {
                        return Ok(());
                    }
                    return Err(err);
                }
            }
        }
    }

    fn detect_resize(&mut self) -> io::Result<()> {
        let size = terminal_size()?;
        if size != self.last_size {
            self.last_size = size;
            self.pending.push_back(Event::Resize {
                width: size.0,
                height: size.1,
            });
        }
        Ok(())
    }
}

impl BackendEventSource for RogerTtyEventSource {
    type Error = io::Error;

    fn size(&self) -> Result<(u16, u16), Self::Error> {
        terminal_size()
    }

    fn set_features(&mut self, _features: BackendFeatures) -> Result<(), Self::Error> {
        // This slice is keyboard-only: no mouse capture, paste, focus, or
        // kitty keyboard modes are enabled, so there is nothing to toggle.
        Ok(())
    }

    fn poll_event(&mut self, timeout: Duration) -> Result<bool, Self::Error> {
        if !self.pending.is_empty() {
            return Ok(true);
        }
        self.detect_resize()?;
        if !self.pending.is_empty() {
            return Ok(true);
        }

        let mut fds = [libc::pollfd {
            fd: STDIN_FD,
            events: libc::POLLIN,
            revents: 0,
        }];
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        // SAFETY: polling one valid fd struct with a bounded timeout.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout_ms) };
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(err);
        }
        if ready == 0 {
            // Idle timeout: flush a pending lone-ESC (or other ambiguous
            // prefix) out of the parser so Escape registers promptly.
            if let Some(event) = self.parser.timeout() {
                self.pending.push_back(event);
                return Ok(true);
            }
            return Ok(false);
        }
        self.drain_stdin()?;
        Ok(!self.pending.is_empty())
    }

    fn read_event(&mut self) -> Result<Option<Event>, Self::Error> {
        if self.pending.is_empty() {
            self.drain_stdin()?;
        }
        Ok(self.pending.pop_front())
    }
}

fn terminal_size() -> io::Result<(u16, u16)> {
    // SAFETY: TIOCGWINSZ fills a zero-initialized winsize for a valid fd.
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((size.ws_col.max(1), size.ws_row.max(1)))
    }
}
