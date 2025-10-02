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

use framebuffer::{Framebuffer, KdMode};
use manager::LoginManager;
use canvas::{Canvas, BasicCanvas};
use framebuffer_backend::FramebufferBackend;
use drm_backend::DrmBackend;
use config::parse_args;

fn main() {
    let config = parse_args();

    let canvas: Box<dyn Canvas>;

    match config.session.get(0).map(|s| s.as_str()) {
        Some("drm") => {
            let drm_backend = DrmBackend::new("/dev/dri/card0").expect("unable to open drm device");
            let basic_canvas = BasicCanvas::new(Box::new(drm_backend));
            canvas = Box::new(basic_canvas);
        }
        _ => {
            let framebuffer = Box::new(Framebuffer::new("/dev/fb0").expect("unable to open framebuffer device"));
            Framebuffer::set_kd_mode(KdMode::Graphics).expect("unable to enter graphics mode");
            let fb_backend = FramebufferBackend::new(Box::leak(framebuffer));
            let basic_canvas = BasicCanvas::new(Box::new(fb_backend));
            canvas = Box::new(basic_canvas);
        }
    }

    let mut login_manager = LoginManager::new(config.clone());
    login_manager.start();

    // Cleanup if framebuffer was used
    if config.session.get(0).map(|s| s.as_str()) != Some("drm") {
        Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    }
}
