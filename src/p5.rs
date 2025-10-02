
use crate::canvas::{Canvas, BasicCanvas};
use crate::framebuffer_backend::FramebufferBackend;
use crate::drm_backend::DrmBackend;
use crate::config::Config;
use framebuffer::{Framebuffer, KdMode};

pub struct P5 {
    canvas: Box<dyn Canvas>,
}

impl P5 {
    pub fn new(config: Config) -> Self {
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
        Self { canvas }
    }

    pub fn background(&mut self, color: u32) {
        self.canvas.clear(color);
    }

    pub fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.canvas.rect(x1, y1, x2, y2, color);
    }

    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.canvas.line(x1, y1, x2, y2, color);
    }

    pub fn ellipse(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        self.canvas.circle(x, y, radius, color);
    }

    pub fn get_screen_size(&self) -> (u32, u32) {
        self.canvas.get_screen_size()
    }

    pub fn run<F1, F2>(&mut self, setup: F1, draw: F2)
    where
        F1: FnOnce(&mut Self),
        F2: Fn(&mut Self),
    {
        setup(self);
        loop {
            draw(self);
        }
    }
}

impl Drop for P5 {
    fn drop(&mut self) {
        // Cleanup if framebuffer was used
        // This is a bit of a hack, we should probably have a better way to determine the backend
        if let Ok(_) = Framebuffer::set_kd_mode(KdMode::Text) {
            //
        }
    }
}
