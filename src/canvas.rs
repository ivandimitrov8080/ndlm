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
        if let Ok(mut subbuf) = self.buf.offset((x, y)) {
            let _ = font.auto_draw_text(&mut subbuf, &self.bg, &color, text);
        }
    }
    fn present(&mut self) {
        // No-op for framebuffer
    }
}

// DRM Renderer implementation
use drm::control::{Device as ControlDevice, ResourceHandle, crtc, framebuffer, plane, connector, ModeTypeFlags, ModeFlags};
use drm::buffer::Buffer as DrmBuffer;
use drm::Device as BasicDevice;
use drm::control::dumbbuffer::DumbBuffer;
use std::fs::File;
use std::os::unix::io::{AsRawFd, RawFd};
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::ptr;

pub struct DrmRenderer {
    pub dev: drm::DeviceFd,
    pub crtc: crtc::Handle,
    pub fb: framebuffer::Handle,
    pub conn: connector::Handle,
    pub width: u32,
    pub height: u32,
    pub map: *mut u8,
    pub size: usize,
    pub stride: u32,
    pub bg: Color,
}

impl DrmRenderer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        use drm::control::*;
        use drm::buffer::DrmFourcc;
        use drm::control::dumbbuffer::DumbMapping;
        use drm::ClientCapability;
        use std::os::fd::FromRawFd;

        // Open the first DRM device
        let dev = drm::DeviceFd::open("/dev/dri/card0", true, true)?;
        let res = dev.resource_handles()?;
        let conn = res.connectors().iter().find_map(|conn| {
            let info = dev.get_connector(*conn).ok()?;
            if info.state() == connector::State::Connected {
                Some(*conn)
            } else {
                None
            }
        }).ok_or("No connected connector")?;
        let conn_info = dev.get_connector(conn)?;
        let mode = *conn_info.modes().get(0).ok_or("No mode found")?;
        let enc = conn_info.current_encoder().or_else(|| conn_info.encoders().get(0).copied()).ok_or("No encoder")?;
        let enc_info = dev.get_encoder(enc)?;
        let crtc = enc_info.crtc().ok_or("No crtc")?;
        let width = mode.size().0;
        let height = mode.size().1;
        // Create dumb buffer
        let dumb = DumbBuffer::create_from_device(&dev, (width as u32, height as u32), 32)?;
        let fb = dev.add_framebuffer(&dumb, 24, 32)?;
        let mut map = dev.map_dumb_buffer(&dumb)?;
        let ptr = map.as_mut_ptr();
        let (w, h) = dumb.size();
        let size = dumb.pitch() as usize * h as usize;
        let stride = dumb.pitch() as u32;
        // Set CRTC
        dev.set_crtc(crtc, Some(fb), (0, 0), &[conn], Some(mode))?;
        Ok(Self {
            dev,
            crtc,
            fb,
            conn,
            width,
            height,
            map: ptr,
            size,
            stride,
            bg: Color::WHITE,
        })
    }

    fn buf(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.map, self.size) }
    }
}

impl Renderer for DrmRenderer {
    fn clear(&mut self, color: Color) {
        self.bg = color;
        let buf = self.buf();
        let val = color.as_argb8888();
        for chunk in buf.chunks_exact_mut(4) {
            chunk.copy_from_slice(&val.to_ne_bytes());
        }
    }
    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>) {
        let buf = self.buf();
        let width = self.width;
        let height = self.height;
        let val = fill.as_argb8888();
        for dx in 0..w {
            for dy in 0..h {
                let px = x + dx;
                let py = y + dy;
                if px < width && py < height {
                    let offset = ((py * width + px) * 4) as usize;
                    buf[offset..offset+4].copy_from_slice(&val.to_ne_bytes());
                }
            }
        }
        if let Some(stroke) = stroke {
            let sval = stroke.as_argb8888();
            for dx in 0..w {
                let px = x + dx;
                if px < width {
                    let top = ((y * width + px) * 4) as usize;
                    let bot = (((y + h - 1) * width + px) * 4) as usize;
                    if y < height { buf[top..top+4].copy_from_slice(&sval.to_ne_bytes()); }
                    if y + h - 1 < height { buf[bot..bot+4].copy_from_slice(&sval.to_ne_bytes()); }
                }
            }
            for dy in 0..h {
                let py = y + dy;
                if py < height {
                    let left = ((py * width + x) * 4) as usize;
                    let right = ((py * width + x + w - 1) * 4) as usize;
                    if x < width { buf[left..left+4].copy_from_slice(&sval.to_ne_bytes()); }
                    if x + w - 1 < width { buf[right..right+4].copy_from_slice(&sval.to_ne_bytes()); }
                }
            }
        }
    }
    fn ellipse(&mut self, x: u32, y: u32, w: u32, h: u32, fill: Color, stroke: Option<Color>) {
        let buf = self.buf();
        let width = self.width;
        let height = self.height;
        let val = fill.as_argb8888();
        let cx = x as i32 + w as i32 / 2;
        let cy = y as i32 + h as i32 / 2;
        let rx = w as i32 / 2;
        let ry = h as i32 / 2;
        for dx in -rx..=rx {
            for dy in -ry..=ry {
                if (dx * dx) * (ry * ry) + (dy * dy) * (rx * rx) <= (rx * rx) * (ry * ry) {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let offset = (((py as u32) * width + (px as u32)) * 4) as usize;
                        buf[offset..offset+4].copy_from_slice(&val.to_ne_bytes());
                    }
                }
            }
        }
        if let Some(stroke) = stroke {
            let sval = stroke.as_argb8888();
            for dx in -rx..=rx {
                for dy in [-ry, ry] {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let offset = (((py as u32) * width + (px as u32)) * 4) as usize;
                        buf[offset..offset+4].copy_from_slice(&sval.to_ne_bytes());
                    }
                }
            }
            for dy in -ry..=ry {
                for dx in [-rx, rx] {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let offset = (((py as u32) * width + (px as u32)) * 4) as usize;
                        buf[offset..offset+4].copy_from_slice(&sval.to_ne_bytes());
                    }
                }
            }
        }
    }
    fn line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, stroke: Color) {
        let buf = self.buf();
        let width = self.width;
        let height = self.height;
        let sval = stroke.as_argb8888();
        let (mut x0, mut y0, x1, y1) = (x1 as i32, y1 as i32, x2 as i32, y2 as i32);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x0 >= 0 && y0 >= 0 && (x0 as u32) < width && (y0 as u32) < height {
                let offset = (((y0 as u32) * width + (x0 as u32)) * 4) as usize;
                buf[offset..offset+4].copy_from_slice(&sval.to_ne_bytes());
            }
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }
    fn text(&mut self, x: u32, y: u32, text: &str, font: &Font, _size: f32, color: Color) {
        // For simplicity, not implemented here. You can port the framebuffer logic if needed.
    }
    fn present(&mut self) {
        // For dumb buffers, the buffer is already mapped and shown after drawing.
        // If page flipping is needed, implement here.
    }
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
