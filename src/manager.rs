#[cfg(feature = "drm")]
use crate::keyboard::drm_input::EvdevInput;
#[cfg(feature = "framebuffer")]
use crate::keyboard::fb_input::StdinInput;
use crate::keyboard::{KeyEvent, KeyboardInput};
use std::fs;

use crate::color::Color;
use framebuffer::{Framebuffer, KdMode};

use crate::{greetd, Config, Error};
const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";

// from linux/fb.h

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
}

use crate::canvas::Canvas; // now generic over lifetime

pub struct LoginManager {
    screen_size: (u32, u32),
    mode: Mode,
    greetd: greetd::GreetD,
    config: Config,
    should_refresh: bool,
    input: Box<dyn KeyboardInput>,
    username: String,
    password: String,
    should_quit: bool,
}

impl LoginManager {
    pub fn new(screen_size: (u32, u32), config: Config, input: Box<dyn KeyboardInput>) -> Self {
        Self {
            screen_size,
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
            should_refresh: false,
            input,
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
            config,
            should_quit: false,
        }
    }

    fn refresh(&mut self) {
        // No-op: handled by Canvas/Renderer now
        self.should_refresh = false;
    }

    fn clear(&mut self, canvas: &mut Canvas<'_>) {
        let bg = self.config.theme.module.background_start_color;
        canvas.background(bg);
        self.should_refresh = true;
    }

    fn draw_prompt(&mut self, canvas: &mut Canvas<'_>, offset: (u32, u32)) -> Result<(), Error> {
        let mut stars = "".to_string();
        for _ in 0..self.password.len() {
            stars += "*";
        }
        let (username_color, password_color) = match self.mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };

        let username = self.username.clone();
        let (x, y) = (offset.0 - 40, offset.1 - 10);

        canvas.fill(username_color);
        canvas.text(&format!("Username: {username}"), x, y);
        canvas.fill(password_color);
        canvas.text(&format!("Password: {stars}"), x, y + 20);

        Ok(())
    }

    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }

    fn draw(&mut self, canvas: &mut Canvas<'_>) {
        let xoff = self.config.theme.module.dialog_horizontal_alignment;
        let yoff = self.config.theme.module.dialog_vertical_alignment;
        let x = (self.screen_size.0 as f32 * xoff) as u32;
        let y = (self.screen_size.1 as f32 * yoff) as u32;
        self.draw_prompt(canvas, (x, y))
            .expect("unable to draw prompt");
        self.should_refresh = true;
    }

    fn handle_keyboard(&mut self) {
        if let Some(event) = self.input.next_key_event() {
            match event {
                KeyEvent::Char(v) => match v {
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
                                        let _ =
                                            fs::write(LAST_USER_USERNAME, self.username.clone());
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
                },
                // Add more KeyEvent variants as needed
            }
        }
    }

    fn setup(&mut self, canvas: &mut Canvas<'_>) {
        self.clear(canvas);
        self.draw(canvas);
        match fs::read_to_string(LAST_USER_USERNAME) {
            Ok(user) => {
                self.username = user;
                self.mode = Mode::EditingPassword;
            }
            Err(_) => {}
        };
    }

    pub fn start(&mut self, canvas: &mut Canvas<'_>) {
        self.setup(canvas);
        loop {
            self.draw(canvas);
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
