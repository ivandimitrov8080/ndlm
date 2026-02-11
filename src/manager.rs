use libc::{POLLIN, POLLPRI, poll, pollfd};
use std::fs;
use std::io::{Bytes, Read, StdinLock};
use std::os::unix::io::AsRawFd;

use crate::color::Color;

use crate::{Config, greetd};
const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";

// from linux/fb.h

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
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
    should_refresh: bool,
    stdin_bytes: Bytes<StdinLock<'static>>,
    username: String,
    password: String,
    should_quit: bool,
    drm_card: Option<&'a crate::manager::Card>, // DRM device handle
    fb_id: u32,
    crtc_id: u32,
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
        Self {
            buf,
            screen_size: (width, height),
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
            should_refresh: false,
            stdin_bytes: std::io::stdin().lock().bytes(),
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
            config,
            should_quit: false,
            drm_card: Some(drm_card),
            fb_id,
            crtc_id,
        }
    }

    fn wait_for_drm_event(&self) {
        if let Some(card) = self.drm_card {
            let fd = card.as_raw_fd();
            let mut fds = [pollfd {
                fd,
                events: (POLLIN | POLLPRI) as i16,
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

    fn refresh(&mut self) {
        self.should_refresh = false;
    }

    fn clear_surface(surf: &crate::draw::FramebufferSurface, bg: &Color, screen_size: (u32, u32)) {
        surf.fill_rect(0, 0, screen_size.0 as i32, screen_size.1 as i32, bg);
    }

    fn draw_prompt_surface(
        surf: &mut crate::draw::FramebufferSurface,
        offset: (u32, u32),
        username: &str,
        password: &str,
        mode: Mode,
        bg: &Color,
    ) {
        let mut stars = "".to_string();
        for _ in 0..password.len() {
            stars += "*";
        }
        let (username_color, password_color) = match mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };
        let (x, y) = (offset.0 - 80, offset.1 - 40);
        surf.fill_input_region(x as i32, y as i32, 320, 56, bg);
        surf.draw_text_region(
            &format!("Username: {username}"),
            "DejaVu Sans Mono 18",
            &username_color,
            0,
        );
        surf.draw_text_region(
            &format!("Password: {stars}"),
            "DejaVu Sans Mono 18",
            &password_color,
            24,
        );
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
        Self::clear_surface(
            &mut mut_surface,
            &self.config.theme.module.background_start_color,
            self.screen_size,
        );
        Self::draw_prompt_surface(
            &mut mut_surface,
            (x, y),
            &self.username,
            &self.password,
            self.mode,
            &self.config.theme.module.background_start_color,
        );
        self.should_refresh = true;
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

    fn read_byte(&mut self) -> u8 {
        self.stdin_bytes
            .next()
            .and_then(Result::ok)
            .unwrap_or_else(|| quit())
    }

    fn handle_keyboard(&mut self) {
        match self.read_byte() as char {
            '\x15' | '\x0B' => match self.mode {
                Mode::EditingUsername => self.username.clear(),
                Mode::EditingPassword => self.password.clear(),
            },
            '\x03' | '\x04' => {
                self.username.clear();
                self.password.clear();
                self.greetd.cancel();
                self.should_quit = true;
                return;
            }
            '\x7F' => match self.mode {
                Mode::EditingUsername => {
                    self.username.pop();
                }
                Mode::EditingPassword => {
                    self.password.pop();
                }
            },
            '\t' => self.goto_next_mode(),
            '\r' => match self.mode {
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
                            self.config.session.clone(),
                        );
                        match res {
                            Ok(_) => {
                                let _ = fs::write(LAST_USER_USERNAME, self.username.clone());
                                self.should_quit = true;
                                return;
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
            v => match self.mode {
                Mode::EditingUsername => self.username.push(v as char),
                Mode::EditingPassword => self.password.push(v as char),
            },
        }
    }

    fn setup(&mut self) {
        let mut_surface = crate::draw::FramebufferSurface::new(self.buf, self.screen_size)
            .expect("could not create framebuffer surface");
        Self::clear_surface(
            &mut_surface,
            &self.config.theme.module.background_start_color,
            self.screen_size,
        );
        self.draw();
        self.wait_for_drm_event(); // Wait for initial flip event
        match fs::read_to_string(LAST_USER_USERNAME) {
            Ok(user) => {
                self.username = user;
                self.mode = Mode::EditingPassword;
            }
            Err(_) => {}
        };
    }

    pub fn start(&mut self) {
        self.setup();
        loop {
            self.draw();
            self.wait_for_drm_event(); // Wait before next draw/flip
            self.handle_keyboard();
            self.refresh();
            if self.should_quit {
                break;
            }
        }
    }
}

fn quit() -> ! {
    std::process::exit(1);
}
