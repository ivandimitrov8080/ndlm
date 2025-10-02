use crate::graphics_backend::GraphicsBackend;

pub trait Canvas {
    fn setup(&mut self);
    fn draw(&mut self);
    fn get_screen_size(&self) -> (u32, u32);
    fn clear(&mut self, color: u32);
}

pub struct BasicCanvas {
    backend: Box<dyn GraphicsBackend>,
}

impl BasicCanvas {
    pub fn new(backend: Box<dyn GraphicsBackend>) -> Self {
        Self { backend }
    }

    pub fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.backend.draw_rect(x1, y1, x2, y2, color);
    }

    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.backend.draw_line(x1, y1, x2, y2, color);
    }

    pub fn circle(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        self.backend.draw_circle(x, y, radius, color);
    }

    pub fn clear(&mut self, color: u32) {
        self.backend.clear(color);
    }
}

impl Canvas for BasicCanvas {
    fn setup(&mut self) {
        // Default empty setup
    }

    fn draw(&mut self) {
        // Default empty draw
    }

    fn get_screen_size(&self) -> (u32, u32) {
        self.backend.get_screen_size()
    }

    fn clear(&mut self, color: u32) {
        self.backend.clear(color);
    }
}
