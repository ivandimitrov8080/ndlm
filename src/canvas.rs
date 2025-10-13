use crate::color::Color;
use crate::draw::Font;

pub trait Renderer {
    fn clear(&mut self, color: Color);
    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>);
    fn ellipse(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>);
    fn line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, stroke: Color);
    fn text(&mut self, x: u32, y: u32, text: &str, font: &Font, size: f32, color: Color);
    fn present(&mut self);
}

use crate::buffer::Buffer;

pub struct FramebufferRenderer<'a> {
    buf: Buffer<'a>,
    bg: Color,
}

impl<'a> FramebufferRenderer<'a> {
    pub fn new(buf: Buffer<'a>) -> Self {
        Self { buf, bg: Color::WHITE }
    }
}

impl<'a> Renderer for FramebufferRenderer<'a> {
    fn clear(&mut self, color: Color) {
        self.bg = color;
        self.buf.memset(&color);
    }
    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>) {
        for dx in 0..w {
            for dy in 0..h {
                let _ = self.buf.put((x + dx, y + dy), &fill);
            }
        }
        if let Some(stroke) = stroke {
            for dx in 0..w {
                let _ = self.buf.put((x + dx, y), &stroke);
                let _ = self.buf.put((x + dx, y + h - 1), &stroke);
            }
            for dy in 0..h {
                let _ = self.buf.put((x, y + dy), &stroke);
                let _ = self.buf.put((x + w - 1, y + dy), &stroke);
            }
        }
    }
    fn ellipse(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>) {
        // Simple bounding-box fill for now
        let cx = x as i32 + w as i32 / 2;
        let cy = y as i32 + h as i32 / 2;
        let rx = w as i32 / 2;
        let ry = h as i32 / 2;
        for dx in -rx..=rx {
            for dy in -ry..=ry {
                if (dx * dx) * (ry * ry) + (dy * dy) * (rx * rx) <= (rx * rx) * (ry * ry) {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 {
                        let _ = self.buf.put((px as u32, py as u32), &fill);
                    }
                }
            }
        }
        // Stroke (optional, just bounding box for now)
        if let Some(stroke) = stroke {
            for dx in -rx..=rx {
                for dy in [-ry, ry] {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 {
                        let _ = self.buf.put((px as u32, py as u32), &stroke);
                    }
                }
            }
            for dy in -ry..=ry {
                for dx in [-rx, rx] {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 {
                        let _ = self.buf.put((px as u32, py as u32), &stroke);
                    }
                }
            }
        }
    }
    fn line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, stroke: Color) {
        // Bresenham's line algorithm
        let (mut x0, mut y0, x1, y1) = (x1 as i32, y1 as i32, x2 as i32, y2 as i32);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x0 >= 0 && y0 >= 0 {
                let _ = self.buf.put((x0 as u32, y0 as u32), &stroke);
            }
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }
    fn text(&mut self, x: u32, y: u32, text: &str, font: &Font, _size: f32, color: Color) {
        let mut font = font.clone();
        let _ = font.auto_draw_text(&mut self.buf, &self.bg, &color, text);
    }
    fn present(&mut self) {
        // No-op for framebuffer
    }
}

// DRM Renderer stub
pub struct DrmRenderer;
impl Renderer for DrmRenderer {
    fn clear(&mut self, _color: Color) {}
    fn rect(&mut self, _x: u32, _y: u32, _w: u32, _h: u32, _fill: Color, _stroke: Option<Color>) {}
    fn ellipse(&mut self, _x: u32, _y: u32, _w: u32, _h: u32, _fill: Color, _stroke: Option<Color>) {}
    fn line(&mut self, _x1: u32, _y1: u32, _x2: u32, _y2: u32, _stroke: Color) {}
    fn text(&mut self, _x: u32, _y: u32, _text: &str, _font: &Font, _size: f32, _color: Color) {}
    fn present(&mut self) {}
}

pub struct Canvas<'a> {
    pub renderer: Box<dyn Renderer + 'a>,
    pub fill: Color,
    pub stroke: Option<Color>,
    pub font: Font,
    pub font_size: f32,
}

impl<'a> Canvas<'a> {
    pub fn background(&mut self, color: Color) { self.renderer.clear(color); }
    pub fn fill(&mut self, color: Color) { self.fill = color; }
    pub fn stroke(&mut self, color: Color) { self.stroke = Some(color); }
    pub fn no_stroke(&mut self) { self.stroke = None; }
    pub fn rect(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.renderer.rect(x, y, w, h, self.fill, self.stroke);
    }
    pub fn ellipse(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.renderer.ellipse(x, y, w, h, self.fill, self.stroke);
    }
    pub fn line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32) {
        if let Some(stroke) = self.stroke {
            self.renderer.line(x1, y1, x2, y2, stroke);
        }
    }
    pub fn text(&mut self, text: &str, x: u32, y: u32) {
        self.renderer.text(x, y, text, &self.font, self.font_size, self.fill);
    }
    pub fn present(&mut self) { self.renderer.present(); }
}
