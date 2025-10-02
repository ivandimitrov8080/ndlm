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

use config::parse_args;
use framebuffer::{Framebuffer, KdMode};

use manager::LoginManager;

use std::io::Read;
use std::sync::mpsc;
use std::thread;

fn main() {
    let config = parse_args();

    let (input_tx, input_rx) = mpsc::channel();

    thread::spawn(move || {
        let stdin = std::io::stdin();
        for byte in stdin.bytes() {
            if let Ok(b) = byte {
                if input_tx.send(b).is_err() {
                    break;
                }
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
