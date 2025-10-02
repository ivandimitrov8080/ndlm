#![deny(rust_2018_idioms)]

mod buffer;
mod canvas;
mod color;
mod config;
mod draw;
mod drm_backend;
mod error;
mod framebuffer_backend;
mod graphics_backend;
mod greetd;
mod manager;
pub mod p5;

use canvas::{BasicCanvas, Canvas};
use config::parse_args;
use drm_backend::DrmBackend;
use framebuffer::{Framebuffer, KdMode};
use framebuffer_backend::FramebufferBackend;
use manager::LoginManager;

use std::io::Read;
use std::sync::mpsc;
use std::thread;

fn main() {
    let config = parse_args();

    let (input_tx, input_rx) = mpsc::channel();

    thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin);
        let mut buffer = [0; 1];
        loop {
            if reader.read(&mut buffer).is_ok() {
                input_tx.send(buffer[0]).unwrap();
            } else {
                break;
            }
        }
    });

    let mut login_manager = LoginManager::new(config.clone(), input_rx);
    login_manager.start();

    // Cleanup if framebuffer was used
    if config.session.get(0).map(|s| s.as_str()) != Some("drm") {
        Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    }
}
