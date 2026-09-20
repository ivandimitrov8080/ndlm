use libc::{POLLIN, POLLPRI, poll, pollfd};
use std::fs::{self, File};
use std::io::{self, ErrorKind, StdinLock};
use std::num::NonZeroU32;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use termion::event::Key;
use termion::input::TermRead;

use drm::control::{connector, crtc, framebuffer};

use crate::draw::FramebufferSurface;
use crate::{Config, draw, greetd, vt};
const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";
const LAST_SESSION_NAME: &str = "/var/cache/ndlm/lastsession";

/// How long to wait for events before re-checking the active VT, in
/// milliseconds. Only ever reached if the kernel did not accept the VT
/// handshake.
const EVENT_POLL_TIMEOUT: libc::c_int = 250;

// from linux/fb.h

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
    pub exec: Vec<String>,
}

fn parse_desktop_entry(path: &Path) -> Option<Session> {
    let entry = match freedesktop_entry_parser::parse_entry(path) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    };

    let name = entry
        .section("Desktop Entry")
        .attr("Name")
        .unwrap_or("No name")
        .to_string();

    let exec_str = match entry.section("Desktop Entry").attr("Exec") {
        Some(v) => v,
        None => panic!("No Exec for desktop entry"),
    };

    let exec = match shell_words::split(exec_str) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    };

    Some(Session { name, exec })
}

fn load_sessions() -> Vec<Session> {
    let mut sessions = Vec::new();
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut dirs = Vec::new();
    for base_dir in xdg_data_dirs.split(':') {
        if !base_dir.is_empty() {
            dirs.push(format!("{}/xsessions", base_dir));
            dirs.push(format!("{}/wayland-sessions", base_dir));
        }
    }

    for dir in &dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop")
                    && let Some(session) = parse_desktop_entry(&path)
                {
                    sessions.push(session);
                }
            }
        }
    }

    sessions.sort_by(|a, b| a.name.cmp(&b.name));
    sessions
}

pub struct Card(pub std::fs::File);

impl std::os::unix::io::AsFd for Card {
    fn as_fd(&self) -> std::os::unix::io::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl std::os::unix::io::AsRawFd for Card {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.0.as_raw_fd()
    }
}

use drm::Device;
impl Device for Card {}

use drm::control::Device as ControlDevice;
impl ControlDevice for Card {}

/// The display ndlm scans out on, plus everything needed to take it over again
/// after a VT switch.
pub struct Display<'a> {
    pub card: &'a Card,
    /// Framebuffer handle.
    pub framebuffer: u32,
    /// CRTC handle.
    pub crtc: u32,
    pub connector: connector::Handle,
    /// The mode the CRTC is set to.
    pub mode: drm::control::Mode,
}

pub struct LoginManager<'a> {
    buf: &'a mut [u8],
    screen_size: (u32, u32),
    mode: Mode,
    greetd: greetd::GreetD,
    config: Config,
    stdin_keys: termion::input::Keys<StdinLock<'static>>,
    username: String,
    password: String,
    should_quit: bool,
    display: Display<'a>,
    sessions: Vec<Session>,
    current_session: Session,
    vt: vt::Vt,
    /// Read end of the pipe the VT signal handler writes to.
    vt_waker: File,
    /// Whether our VT is the active one, and therefore ours to render on.
    vt_active: bool,
    /// Whether the buffer has been changed and needs to be scanned out again.
    dirty: bool,
    /// Last failure to scan out, remembered to keep the retries quiet.
    flip_error: Option<String>,
}

impl<'a> LoginManager<'a> {
    pub fn new(
        buf: &'a mut [u8],
        config: Config,
        width: u32,
        height: u32,
        display: Display<'a>,
        vt: vt::Vt,
    ) -> Self {
        let mut sessions = load_sessions();
        if sessions.is_empty() && !config.session.is_empty() {
            sessions.push(Session {
                name: "Default".to_string(),
                exec: config.session.clone(),
            });
        }

        let selected_session_idx = if !config.session.is_empty() {
            let config_session_name = config
                .session
                .first()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();

            sessions
                .iter()
                .position(|s| s.name.to_lowercase().contains(&config_session_name))
                .unwrap_or(0)
        } else {
            0
        };

        let current_session = sessions
            .get(selected_session_idx)
            .cloned()
            .unwrap_or_else(|| Session {
                name: "Default".to_string(),
                exec: config.session.clone(),
            });

        // Has to happen before the VT is put into VT_PROCESS mode, otherwise
        // the first VT switch away from it kills us.
        let vt_waker = match vt::signal_waker() {
            Ok(waker) => waker,
            Err(e) => {
                eprintln!("unable to listen for VT switches: {e}");
                std::process::exit(1);
            }
        };

        Self {
            buf,
            screen_size: (width, height),
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
            stdin_keys: std::io::stdin().lock().keys(),
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
            config,
            should_quit: false,
            display,
            sessions,
            current_session,
            vt,
            vt_waker,
            vt_active: false,
            dirty: true,
            flip_error: None,
        }
    }

    /// Waits until there is something to do: a finished page flip, a key press
    /// or a VT switch. Returns whether there is input waiting to be read.
    fn wait_for_event(&self) -> bool {
        let mut fds = [
            pollfd {
                fd: self.display.card.as_raw_fd(),
                events: (POLLIN | POLLPRI),
                revents: 0,
            },
            pollfd {
                fd: libc::STDIN_FILENO,
                events: POLLIN,
                revents: 0,
            },
            pollfd {
                fd: self.vt_waker.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            },
        ];

        let res = unsafe {
            poll(
                fds.as_mut_ptr(),
                fds.len() as libc::nfds_t,
                EVENT_POLL_TIMEOUT,
            )
        };
        if res < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != ErrorKind::Interrupted {
                eprintln!("poll() error while waiting for events: {e}");
            }
            return false;
        }

        if fds[2].revents != 0 {
            vt::drain_waker(&self.vt_waker);
        }

        if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            eprintln!("the DRM device is gone, giving up");
            quit();
        }
        if fds[0].revents & (POLLIN | POLLPRI) != 0 {
            let _ = self.display.card.receive_events();
        }

        let input_available = fds[1].revents & POLLIN != 0;
        // A hung up terminal stays readable, so polling it again would spin.
        if !input_available && fds[1].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            quit();
        }

        input_available
    }

    /// Keeps the display in sync with the VT ndlm is meant to run on: it is
    /// only rendered on while that VT is the active one.
    fn sync_vt(&mut self) {
        let (release_requested, acquire_requested) = vt::take_switch_requests();

        if release_requested {
            // The kernel blocks the switch until we allow it, so hand the
            // display over first and complete the switch afterwards: only then
            // can the VT being switched to paint itself.
            if self.vt_active {
                self.leave_vt();
            }
            if let Err(e) = self.vt.ack_release() {
                eprintln!("unable to release {}: {e}", self.vt.name());
            }
        }

        if acquire_requested {
            if let Err(e) = self.vt.ack_acquire() {
                eprintln!("unable to acknowledge {}: {e}", self.vt.name());
            }
            if !self.vt_active {
                self.enter_vt();
            }
        }

        // Without a signal to act on, make sure we are not out of sync anyway
        // (for example when the VT handshake could not be set up).
        if !release_requested && !acquire_requested {
            match self.vt.is_active() {
                Ok(true) if !self.vt_active => self.enter_vt(),
                Ok(false) if self.vt_active => self.leave_vt(),
                _ => {}
            }
        }
    }

    /// Takes over the display on our VT.
    fn enter_vt(&mut self) {
        if let Err(e) = self.display.card.acquire_master_lock() {
            eprintln!("unable to acquire DRM master: {e}");
        }
        if let Err(e) = self.vt.set_graphics_mode() {
            eprintln!("unable to switch {} to graphics mode: {e}", self.vt.name());
        }
        // The kernel console has been using the CRTC while we were away, so it
        // has to be pointed at our framebuffer again.
        self.modeset();
        self.vt_active = true;
        self.dirty = true;
    }

    /// Hands the display back so the kernel console can paint the VT the user
    /// switched to. The VT stays in graphics mode: it is still ours.
    fn leave_vt(&mut self) {
        if let Err(e) = self.display.card.release_master_lock() {
            eprintln!("unable to release DRM master: {e}");
        }
        self.vt_active = false;
    }

    /// Points the CRTC at our framebuffer.
    fn modeset(&self) {
        let crtc = crtc::Handle::from(
            NonZeroU32::new(self.display.crtc).expect("CRTC id must be nonzero"),
        );
        let fb = framebuffer::Handle::from(
            NonZeroU32::new(self.display.framebuffer).expect("FB id must be nonzero"),
        );

        if let Err(e) = self.display.card.set_crtc(
            crtc,
            Some(fb),
            (0, 0),
            &[self.display.connector],
            Some(self.display.mode),
        ) {
            eprintln!("unable to set up the display: {e}");
        }
    }

    fn clear_surface(&self, surf: &FramebufferSurface) {
        let module = &self.config.theme.module;
        let (start, end) =
            draw::background_colors(module.background_start_color, module.background_end_color);
        surf.fill_vertical_gradient(
            0,
            0,
            self.screen_size.0 as i32,
            self.screen_size.1 as i32,
            &start,
            &end,
        );
    }

    /// Fills the buffer with the background colour without touching the display.
    fn clear(&mut self) {
        let surface = FramebufferSurface::new(self.buf, self.screen_size)
            .expect("could not create framebuffer surface");
        self.clear_surface(&surface);
    }

    fn draw_prompt_surface(&self, surf: &mut FramebufferSurface, center: (u32, u32)) {
        let module = &self.config.theme.module;
        let mut style = draw::PromptStyle::from_backgrounds(
            module.background_start_color,
            module.background_end_color,
        );
        if !module.title_font.to_string().is_empty() {
            style.title_font = module.title_font.clone();
        }

        let title = "Welcome".to_string();
        let password = "•".repeat(self.password.len());
        let session = if self.sessions.is_empty() {
            None
        } else if self.sessions.len() > 1 {
            Some(format!("←  {}  →", self.current_session.name))
        } else {
            Some(self.current_session.name.clone())
        };

        let prompt = draw::Prompt {
            title: &title,
            username: &self.username,
            password: &password,
            session: session.as_deref(),
            focused: match self.mode {
                Mode::EditingUsername => draw::Field::Username,
                Mode::EditingPassword => draw::Field::Password,
            },
            center,
            screen: self.screen_size,
        };

        draw::draw_prompt(surf, &prompt, &style);
    }

    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }

    /// Renders the current state and scans it out. Only called while our VT is
    /// the active one.
    fn draw(&mut self) {
        let xoff = self.config.theme.module.dialog_horizontal_alignment;
        let yoff = self.config.theme.module.dialog_vertical_alignment;
        let x = (self.screen_size.0 as f32 * xoff) as u32;
        let y = (self.screen_size.1 as f32 * yoff) as u32;
        let mut surface = FramebufferSurface::new(self.buf, self.screen_size)
            .expect("could not create framebuffer surface");
        self.clear_surface(&surface);
        self.draw_prompt_surface(&mut surface, (x, y));

        let crtc = crtc::Handle::from(
            NonZeroU32::new(self.display.crtc).expect("CRTC id must be nonzero"),
        );
        let fb = framebuffer::Handle::from(
            NonZeroU32::new(self.display.framebuffer).expect("FB id must be nonzero"),
        );
        match self
            .display
            .card
            .page_flip(crtc, fb, drm::control::PageFlipFlags::EVENT, None)
        {
            Ok(()) => {
                self.dirty = false;
                self.flip_error = None;
            }
            Err(e) => {
                // Not fatal, and not necessarily permanent: another process can
                // still be holding DRM master, for example a compositor that is
                // in the middle of shutting down. Keep retrying, but do not
                // repeat the same complaint over and over.
                let error = e.to_string();
                if self.flip_error.as_deref() != Some(error.as_str()) {
                    eprintln!("DRM page flip failed: {e}");
                    self.flip_error = Some(error);
                }
                self.dirty = true;
            }
        }
    }

    /// Reads one key from the terminal and applies it. Only called when input
    /// is available, i.e. when our VT is the active one.
    fn handle_keyboard(&mut self) {
        let key = match self.stdin_keys.next() {
            Some(Ok(key)) => key,
            // A VT switch interrupts an ongoing read; there is nothing to do.
            Some(Err(e)) if e.kind() == ErrorKind::Interrupted => return,
            Some(Err(e)) => {
                eprintln!("unable to read from the terminal: {e}");
                return;
            }
            None => quit(),
        };

        let changed = match key {
            Key::Left => {
                if let Some(pos) = self
                    .sessions
                    .iter()
                    .position(|s| s.name == self.current_session.name)
                {
                    let new_idx = if pos == 0 {
                        self.sessions.len() - 1
                    } else {
                        pos - 1
                    };
                    self.current_session = self.sessions[new_idx].clone();
                    true
                } else {
                    false
                }
            }
            Key::Right => {
                if let Some(pos) = self
                    .sessions
                    .iter()
                    .position(|s| s.name == self.current_session.name)
                {
                    let new_idx = if pos + 1 == self.sessions.len() {
                        0
                    } else {
                        pos + 1
                    };
                    self.current_session = self.sessions[new_idx].clone();
                    true
                } else {
                    false
                }
            }
            Key::Ctrl('c') | Key::Ctrl('d') => {
                self.username.clear();
                self.password.clear();
                self.greetd.cancel();
                self.should_quit = true;
                true
            }
            Key::Backspace => match self.mode {
                Mode::EditingUsername => self.username.pop().is_some(),
                Mode::EditingPassword => self.password.pop().is_some(),
            },
            Key::Char('\t') => {
                self.goto_next_mode();
                true
            }
            Key::Char('\n') => match self.mode {
                Mode::EditingUsername => {
                    if !self.username.is_empty() {
                        self.mode = Mode::EditingPassword;
                        true
                    } else {
                        false
                    }
                }
                Mode::EditingPassword => {
                    if self.password.is_empty() {
                        self.username.clear();
                        self.mode = Mode::EditingUsername;
                    } else {
                        let res = self.greetd.login(
                            self.username.clone(),
                            self.password.clone(),
                            self.current_session.exec.clone(),
                        );
                        match res {
                            Ok(_) => {
                                let _ = fs::write(LAST_USER_USERNAME, self.username.clone());
                                let _ =
                                    fs::write(LAST_SESSION_NAME, self.current_session.name.clone());
                                self.should_quit = true;
                            }
                            Err(_) => {
                                self.username = String::with_capacity(USERNAME_CAP);
                                self.password = String::with_capacity(PASSWORD_CAP);
                                self.mode = Mode::EditingUsername;
                                self.greetd.cancel();
                            }
                        }
                    }
                    true
                }
            },
            Key::Char(v) => match self.mode {
                Mode::EditingUsername => {
                    self.username.push(v);
                    true
                }
                Mode::EditingPassword => {
                    self.password.push(v);
                    true
                }
            },
            _ => false, // Ignore other keys
        };

        if changed {
            self.dirty = true;
        }
    }

    fn setup(&mut self) {
        // Fill the buffer before the CRTC starts scanning out of it, so that
        // taking over the display does not flash garbage.
        self.clear();

        // Ask the kernel to consult us before switching away from our VT.
        if let Err(e) = self.vt.take_process_control() {
            eprintln!("unable to take control of {}: {e}", self.vt.name());
        }

        let active = match self.vt.is_active() {
            Ok(active) => active,
            Err(e) => {
                eprintln!("unable to determine the active VT: {e}");
                true
            }
        };
        if active {
            self.enter_vt();
        }
        // Otherwise greetd was configured not to switch to our VT: rendering is
        // postponed until the kernel reports that it became the active one.

        if let Ok(user) = fs::read_to_string(LAST_USER_USERNAME) {
            self.username = user;
            self.mode = Mode::EditingPassword;
        };
        if self.config.session.is_empty()
            && let Ok(session_name) = fs::read_to_string(LAST_SESSION_NAME)
            && let Some(session) = self.sessions.iter().find(|s| s.name == session_name)
        {
            self.current_session = session.clone();
        };
    }

    /// Gives the VT and the display back, so that whatever runs next can use
    /// them.
    fn teardown(&mut self) {
        if let Err(e) = self.vt.release_process_control() {
            eprintln!("unable to restore VT switching: {e}");
        }
        // Text mode first, then the master lock: the kernel console takes the
        // VT over as soon as it can drive the display again.
        if let Err(e) = self.vt.set_text_mode() {
            eprintln!("unable to switch {} to text mode: {e}", self.vt.name());
        }
        if self.vt_active {
            self.leave_vt();
        }
    }

    pub fn start(&mut self) {
        self.setup();
        loop {
            self.sync_vt();

            if self.vt_active && self.dirty {
                self.draw();
            }

            if self.wait_for_event() {
                self.handle_keyboard();
            }

            if self.should_quit {
                break;
            }
        }
        self.teardown();
    }
}

fn quit() -> ! {
    std::process::exit(1);
}
