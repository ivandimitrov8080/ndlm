use crate::graphics_backend::GraphicsBackend;

pub trait Canvas {
    fn get_screen_size(&self) -> (u32, u32);
    fn clear(&mut self, color: u32);
    fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u32);
    fn cleanup(&mut self);
}

pub struct BasicCanvas {
    backend: Box<dyn GraphicsBackend>,
}

impl BasicCanvas {
    pub fn new(backend: Box<dyn GraphicsBackend>) -> Self {
        Self { backend }
    }

    fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.backend.draw_rect(x1, y1, x2, y2, color);
    }

    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.backend.draw_line(x1, y1, x2, y2, color);
    }

    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        self.backend.draw_circle(x, y, radius, color);
    }

    fn clear(&mut self, color: u32) {
        self.backend.clear(color);
    }
}

impl Canvas for BasicCanvas {
    fn get_screen_size(&self) -> (u32, u32) {
        self.backend.get_screen_size()
    }

    fn clear(&mut self, color: u32) {
        self.clear(color);
    }

    fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.rect(x1, y1, x2, y2, color);
    }

    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.line(x1, y1, x2, y2, color);
    }

    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        self.circle(x, y, radius, color);
    }

    fn cleanup(&mut self) {
        self.backend.cleanup();
    }
}
