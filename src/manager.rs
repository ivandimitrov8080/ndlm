use libc::{POLLIN, POLLPRI, poll, pollfd};
use pango::FontDescription;
use std::fs;
use std::io::StdinLock;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use termion::event::Key;
use termion::input::TermRead;

use crate::color::Color;

use crate::{Config, greetd};
const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";
const LAST_SESSION_NAME: &str = "/var/cache/ndlm/lastsession";

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
    drm_card: Option<&'a crate::manager::Card>, // DRM device handle
    fb_id: u32,
    crtc_id: u32,
    sessions: Vec<Session>,
    current_session: Session,
}

impl<'a> LoginManager<'a> {
    pub fn new(
        buf: &'a mut [u8],
        config: Config,
        width: u32,
        height: u32,
        drm_card: &'a crate::manager::Card,
        fb_id: u32,
        crtc_id: u32,
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
            drm_card: Some(drm_card),
            fb_id,
            crtc_id,
            sessions,
            current_session,
        }
    }

    fn wait_for_drm_event(&self) {
        if let Some(card) = self.drm_card {
            let fd = card.as_raw_fd();
            let mut fds = [pollfd {
                fd,
                events: (POLLIN | POLLPRI),
                revents: 0,
            }];
            let res = unsafe { poll(fds.as_mut_ptr(), 1, -1) }; // -1 = infinite timeout
            if res > 0 && (fds[0].revents & (POLLIN | POLLPRI)) != 0 {
                let _ = card.receive_events();
            } else if res < 0 {
                eprintln!("poll() error while waiting for drm event");
            }
        }
    }

    fn clear_surface(&self, surf: &crate::draw::FramebufferSurface) {
        let bg = &self.config.theme.module.background_start_color;
        surf.fill_rect(
            0,
            0,
            self.screen_size.0 as i32,
            self.screen_size.1 as i32,
            bg,
        );
    }

    fn draw_prompt_surface(&self, surf: &mut crate::draw::FramebufferSurface, offset: (u32, u32)) {
        let stars = "*".repeat(self.password.len());
        let font = FontDescription::from_string("DejaVu Sans Mono 18");
        let font_small = FontDescription::from_string("DejaVu Sans Mono 14");
        let (username_color, password_color) = match self.mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };
        let (x, y) = (offset.0 - 120, offset.1 - 40);

        let bg = &self.config.theme.module.background_start_color;
        surf.fill_input_region(x as i32, y as i32, 480, 90, bg);
        surf.draw_text_region(
            &format!("Username: {}", self.username),
            &font,
            &username_color,
            0,
        );
        surf.draw_text_region(&format!("Password: {stars}"), &font, &password_color, 24);

        // Draw horizontal session list
        if !self.sessions.is_empty() {
            let session_y_offset = 56 + 10; // 10px below password field

            if self.sessions.len() == 1 {
                let text = format!("Session: {}", self.current_session.name);
                surf.draw_text_region(&text, &font_small, &Color::YELLOW, session_y_offset);
            } else {
                let text = format!("Session (←/→): {}", self.current_session.name);
                surf.draw_text_region(&text, &font_small, &Color::YELLOW, session_y_offset);
            }
        }

        surf.composite_region_to_fb();
    }

    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }

    fn draw(&mut self) {
        let xoff = self.config.theme.module.dialog_horizontal_alignment;
        let yoff = self.config.theme.module.dialog_vertical_alignment;
        let x = (self.screen_size.0 as f32 * xoff) as u32;
        let y = (self.screen_size.1 as f32 * yoff) as u32;
        let mut mut_surface = crate::draw::FramebufferSurface::new(self.buf, self.screen_size)
            .expect("could not create framebuffer surface");
        self.clear_surface(&mut_surface);
        self.draw_prompt_surface(&mut mut_surface, (x, y));
        if let Some(card) = self.drm_card {
            use drm::control::Device as _;
            card.page_flip(
                drm::control::crtc::Handle::from(
                    std::num::NonZeroU32::new(self.crtc_id).expect("CRTC id must be nonzero"),
                ),
                drm::control::framebuffer::Handle::from(
                    std::num::NonZeroU32::new(self.fb_id).expect("FB id must be nonzero"),
                ),
                drm::control::PageFlipFlags::EVENT,
                None,
            )
            .expect("DRM page flip failed");
        }
    }

    fn handle_keyboard(&mut self) {
        let key = self
            .stdin_keys
            .next()
            .and_then(Result::ok)
            .unwrap_or_else(|| quit());

        match key {
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
                }
            }
            Key::Ctrl('c') | Key::Ctrl('d') => {
                self.username.clear();
                self.password.clear();
                self.greetd.cancel();
                self.should_quit = true;
            }
            Key::Backspace => match self.mode {
                Mode::EditingUsername => {
                    self.username.pop();
                }
                Mode::EditingPassword => {
                    self.password.pop();
                }
            },
            Key::Char('\t') => self.goto_next_mode(),
            Key::Char('\n') => match self.mode {
                Mode::EditingUsername => {
                    if !self.username.is_empty() {
                        self.mode = Mode::EditingPassword;
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
                }
            },
            Key::Char(v) => match self.mode {
                Mode::EditingUsername => self.username.push(v),
                Mode::EditingPassword => self.password.push(v),
            },
            _ => {} // Ignore other keys
        }
    }

    fn setup(&mut self) {
        let mut_surface = crate::draw::FramebufferSurface::new(self.buf, self.screen_size)
            .expect("could not create framebuffer surface");
        self.clear_surface(&mut_surface);
        self.draw();
        self.wait_for_drm_event(); // Wait for initial flip event
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

    pub fn start(&mut self) {
        self.setup();
        loop {
            self.draw();
            self.wait_for_drm_event(); // Wait before next draw/flip
            self.handle_keyboard();
            if self.should_quit {
                break;
            }
        }
    }
}

fn quit() -> ! {
    std::process::exit(1);
}
