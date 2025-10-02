use crate::buffer::Buffer;
use crate::color::Color;
use crate::graphics_backend::GraphicsBackend;
use framebuffer::Framebuffer;

pub struct FramebufferBackend<'a> {
    buffer: Buffer<'a>,
    fb: &'a mut Framebuffer,
}

impl<'a> FramebufferBackend<'a> {
    pub fn new(fb: &'a mut Framebuffer) -> Self {
        let buffer = unsafe {
            let ptr = fb.frame.as_mut_ptr();
            let len = fb.frame.len();
            Buffer::new(std::slice::from_raw_parts_mut(ptr, len), (fb.var_screen_info.xres, fb.var_screen_info.yres))
        };
        Self { fb, buffer }
    }
}

impl<'a> GraphicsBackend for FramebufferBackend<'a> {
    fn draw_pixel(&mut self, x: i32, y: i32, color: u32) {
        let color_struct = Color::from_argb8888(color);
        let _ = self.buffer.put((x as u32, y as u32), &color_struct);
    }

    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        // Bresenham's line algorithm
        let mut x1 = x1;
        let mut y1 = y1;
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.draw_pixel(x1, y1, color);
            if x1 == x2 && y1 == y2 { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x1 += sx;
            }
            if e2 <= dx {
                err += dx;
                y1 += sy;
            }
        }
    }

    fn draw_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        for x in x1..=x2 {
            self.draw_pixel(x, y1, color);
            self.draw_pixel(x, y2, color);
        }
        for y in y1..=y2 {
            self.draw_pixel(x1, y, color);
            self.draw_pixel(x2, y, color);
        }
    }

    fn draw_circle(&mut self, x: i32, y: i32, radius: i32, color: u32) {
        // Midpoint circle algorithm
        let mut x0 = radius;
        let mut y0 = 0;
        let mut err = 0;

        while x0 >= y0 {
            self.draw_pixel(x + x0, y + y0, color);
            self.draw_pixel(x + y0, y + x0, color);
            self.draw_pixel(x - y0, y + x0, color);
            self.draw_pixel(x - x0, y + y0, color);
            self.draw_pixel(x - x0, y - y0, color);
            self.draw_pixel(x - y0, y - x0, color);
            self.draw_pixel(x + y0, y - x0, color);
            self.draw_pixel(x + x0, y - y0, color);

            y0 += 1;
            if err <= 0 {
                err += 2 * y0 + 1;
            } else {
                x0 -= 1;
                err -= 2 * x0 + 1;
            }
        }
    }

    fn clear(&mut self, color: u32) {
        let color_struct = Color::from_argb8888(color);
        self.buffer.memset(&color_struct);
    }

    fn get_screen_size(&self) -> (u32, u32) {
        (self.fb.var_screen_info.xres, self.fb.var_screen_info.yres)
    }
}
