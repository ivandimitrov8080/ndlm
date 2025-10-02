use std::io::{Read, StdinLock};
use std::{fs, io::Bytes};
use framebuffer::{Framebuffer, KdMode};
use crate::p5::P5;
use crate::color::Color;
use crate::config::Config;
use crate::error::Error;
use crate::greetd;

const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;
const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";


#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
}
pub struct LoginManager {
    p5: P5,
    config: Config,
    should_refresh: bool,
    stdin_bytes: Bytes<StdinLock<'static>>,
    username: String,
    password: String,
    should_quit: bool,
    mode: Mode,
    greetd: greetd::GreetD,
}
impl LoginManager {
    pub fn new(config: Config) -> Self {
        let p5 = P5::new(config.clone());
        Self {
            p5,
            config,
            should_refresh: false,
            stdin_bytes: std::io::stdin().lock().bytes(),
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
            should_quit: false,
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
        }
    }
    fn refresh(&mut self) {
        if self.should_refresh {
            self.should_refresh = false;
            // let mut screeninfo = self.var_screen_info.clone();
            // screeninfo.activate |= FB_ACTIVATE_NOW | FB_ACTIVATE_FORCE;
            // Framebuffer::put_var_screeninfo(self.device, &screeninfo)
            //     .expect("Failed to refresh framebuffer");
        }
    }
    fn clear(&mut self) {
        let bg = self.config.theme.module.background_start_color;
        self.p5.background(bg.as_argb8888());
        self.should_refresh = true;
    }
    fn draw_prompt(&mut self, offset: (u32, u32)) -> Result<(), Error> {
        let bg = self.config.theme.module.background_start_color;
        self.p5.background(bg.as_argb8888());
        let (_username_color, _password_color) = match self.mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };
        let (x, y) = (offset.0 as i32, offset.1 as i32);

        // Draw username box
        self.p5.rect(x - 100, y - 20, x + 100, y + 10, 0xFFFFFF);
        // Draw password box
        self.p5.rect(x - 100, y + 20, x + 100, y + 50, 0xFFFFFF);

        Ok(())
    }
    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }
    fn draw(&mut self) {
        let screen_size = self.p5.get_screen_size();
        let xoff = self.config.theme.module.dialog_horizontal_alignment;
        let yoff = self.config.theme.module.dialog_vertical_alignment;
        let x = (screen_size.0 as f32 * xoff) as u32;
        let y = (screen_size.1 as f32 * yoff) as u32;
        self.draw_prompt((x, y)).expect("unable to draw prompt");
        self.should_refresh = true;
    }
    fn read_byte(&mut self) -> u8 {
        self.stdin_bytes
            .next()
            .and_then(Result::ok)
            .unwrap_or_else(quit)
    }
    fn handle_keyboard(&mut self) {
        match self.read_byte() as char {
            '\x15' | '\x0B' => match self.mode {
                // ctrl-k/ctrl-u
                Mode::EditingUsername => self.username.clear(),
                Mode::EditingPassword => self.password.clear(),
            },
            '\x03' | '\x04' => {
                // ctrl-c/ctrl-D
                self.username.clear();
                self.password.clear();
                self.greetd.cancel();
                self.should_quit = true;
                return;
            }
            '\x7F' => match self.mode {
                // backspace
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
        self.clear();
        self.draw();
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
            self.handle_keyboard();
            self.refresh();
            if self.should_quit {
                return;
            }
        }
    }
}
fn quit() -> u8 {
    Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    std::process::exit(1);
}
