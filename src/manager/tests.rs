#![cfg(test)]
use crate::greetd;
use crate::manager::{LoginManager, Mode, PASSWORD_CAP, USERNAME_CAP};
use crate::p5::P5;
use crate::{canvas::Canvas, config::Config};
use std::sync::mpsc;

struct MockCanvas;

impl Canvas for MockCanvas {
    fn get_screen_size(&self) -> (u32, u32) {
        (800, 600)
    }
    fn clear(&mut self, _color: u32) {}
    fn rect(&mut self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _color: u32) {}
    fn line(&mut self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _color: u32) {}
    fn circle(&mut self, _x: i32, _y: i32, _radius: i32, _color: u32) {}
    fn cleanup(&mut self) {}
}

use std::env;
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn new_test_manager(input: &'static str) -> LoginManager {
    let config = Config::default();
    let (input_tx, input_rx) = mpsc::channel();

    for byte in input.as_bytes() {
        input_tx.send(*byte).unwrap();
    }

    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let socket_path = format!("/tmp/greetd.{}.sock", since_the_epoch.as_nanos());
    env::set_var("GREETD_SOCK", &socket_path);

    let listener = UnixListener::bind(&socket_path).unwrap();

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(_stream) => {}
                Err(_err) => {
                    break;
                }
            }
        }
    });

    LoginManager {
        p5: P5::new_with_canvas(Box::new(MockCanvas)),
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

#[test]
fn test_username_input() {
    let mut login_manager = new_test_manager("testuser");

    for _ in 0.."testuser".len() {
        login_manager.handle_keyboard();
    }

    assert_eq!(login_manager.username, "testuser");
}

#[test]
fn test_password_input() {
    let mut login_manager = new_test_manager("testpass");
    login_manager.mode = Mode::EditingPassword;

    for _ in 0.."testpass".len() {
        login_manager.handle_keyboard();
    }

    assert_eq!(login_manager.password, "testpass");
}

#[test]
fn test_input_mode_switching() {
    let mut login_manager = new_test_manager("testuser\rtestpass");

    for _ in 0.."testuser\rtestpass".len() {
        login_manager.handle_keyboard();
    }

    assert_eq!(login_manager.username, "testuser");
    assert_eq!(login_manager.password, "testpass");
    assert_eq!(login_manager.mode, Mode::EditingPassword);
}
