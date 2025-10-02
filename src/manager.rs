use crate::color::Color;
use crate::config::Config;
use crate::error::Error;
use crate::greetd;
use crate::p5::P5;

use std::fs;
use std::sync::mpsc::Receiver;

const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;
const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";

#[derive(PartialEq, Copy, Clone, Debug)]
enum Mode {
    EditingUsername,
    EditingPassword,
}
pub struct LoginManager {
    p5: P5,
    config: Config,
    should_refresh: bool,
    username: String,
    password: String,
    should_quit: bool,
    mode: Mode,
    greetd: greetd::GreetD,
    input_rx: Receiver<u8>,
}
impl LoginManager {
    pub fn new(config: Config, input_rx: Receiver<u8>) -> Self {
        let p5 = P5::new(config.clone());
        Self {
            p5,
            config,
            should_refresh: false,
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
            should_quit: false,
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
            input_rx,
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
        let prompt_font = self.config.theme.module.font.clone();
        let mut stars = "".to_string();
        for _ in 0..self.password.len() {
            stars += "*";
        }
        let (username_color, password_color) = match self.mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };
        let username = self.username.clone();
        let (x, y) = (offset.0 as i32, offset.1 as i32);

        prompt_font.draw_text(
            &mut self.p5,
            x,
            y,
            &format!("Username: {}", username),
            username_color.as_argb8888(),
        );
        prompt_font.draw_text(
            &mut self.p5,
            x,
            y + 20,
            &format!("Password: {}", stars),
            password_color.as_argb8888(),
        );

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
    fn read_byte(&mut self) -> Option<u8> {
        self.input_rx.try_recv().ok()
    }
    fn handle_keyboard(&mut self) {
        if let Some(byte) = self.read_byte() {
            match byte as char {
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


mod tests;
