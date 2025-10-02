pub trait GraphicsBackend {
    fn draw_pixel(&mut self, x: i32, y: i32, color: u32);
    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
    fn draw_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
    fn draw_circle(&mut self, x: i32, y: i32, radius: i32, color: u32);
    fn clear(&mut self, color: u32);
    fn get_screen_size(&self) -> (u32, u32);
}
