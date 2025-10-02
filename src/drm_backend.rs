use std::fs::File;
use std::os::unix::io::{AsFd, BorrowedFd};

use crate::graphics_backend::GraphicsBackend;
use drm::buffer::DrmFourcc;
use drm::control::dumbbuffer::{DumbBuffer, DumbMapping};
use drm::control::{connector, crtc, framebuffer, Device as ControlDevice, Mode, ResourceHandles};
use drm::Device as BasicDevice;

struct DrmDevice {
    device: File,
}

impl AsFd for DrmDevice {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.device.as_fd()
    }
}

impl BasicDevice for DrmDevice {}
impl ControlDevice for DrmDevice {}

#[allow(dead_code)]
pub struct DrmBackend<'a> {
    device: DrmDevice,
    handles: ResourceHandles,
    connector: connector::Handle,
    crtc: crtc::Handle,
    mode: Mode,
    db: Box<DumbBuffer>,
    fb: framebuffer::Handle,
    map: DumbMapping<'a>,
}

impl<'a> DrmBackend<'a> {
    pub fn new(path: &str) -> Result<Self, std::io::Error> {
        let device = DrmDevice {
            device: File::open(path)?,
        };
        let handles = device.resource_handles().unwrap();

        let connector = handles
            .connectors()
            .iter()
            .find(|conn| {
                let info = device.get_connector(**conn, false).unwrap();
                info.state() == connector::State::Connected
            })
            .unwrap();

        let info = device.get_connector(*connector, false).unwrap();
        let encoder = device
            .get_encoder(*info.encoders().first().unwrap())
            .unwrap();
        let crtc = device.get_crtc(encoder.crtc().unwrap()).unwrap();
        let mode = info.modes()[0];

        let (w, h) = mode.size();
        let db = Box::new(
            device
                .create_dumb_buffer((w as u32, h as u32), DrmFourcc::Xrgb8888, 32)
                .unwrap(),
        );
        let fb = device.add_framebuffer(&*db, 24, 32).unwrap();

        device
            .set_crtc(crtc.handle(), Some(fb), (0, 0), &[*connector], Some(mode))
            .unwrap();

        let map = device.map_dumb_buffer(Box::leak(db.clone())).unwrap();

        Ok(Self {
            device,
            handles: handles.clone(),
            connector: *connector,
            crtc: crtc.handle(),
            mode,
            db,
            fb,
            map,
        })
    }
}

impl<'a> Drop for DrmBackend<'a> {
    fn drop(&mut self) {
        self.device.destroy_framebuffer(self.fb).unwrap();
        self.device.destroy_dumb_buffer(*self.db).unwrap();
    }
}

impl<'a> GraphicsBackend for DrmBackend<'a> {
    fn draw_pixel(&mut self, x: i32, y: i32, color: u32) {
        let (w, _) = self.mode.size();
        let w = w as usize;
        let x = x as usize;
        let y = y as usize;

        let offset = y * w + x;
        self.map[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
    }

    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        let mut x = x1;
        let mut y = y1;
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = if dx > dy { dx } else { -dy } / 2;
        let mut err2;

        loop {
            self.draw_pixel(x, y, color);
            if x == x2 && y == y2 {
                break;
            }
            err2 = err;
            if err2 > -dx {
                err -= dy;
                x += sx;
            }
            if err2 < dy {
                err += dx;
                y += sy;
            }
        }
    }

    fn draw_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        for y in y1..y2 {
            for x in x1..x2 {
                self.draw_pixel(x, y, color);
            }
        }
    }

    fn draw_circle(&mut self, x0: i32, y0: i32, radius: i32, color: u32) {
        let mut x = radius;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            self.draw_pixel(x0 + x, y0 + y, color);
            self.draw_pixel(x0 + y, y0 + x, color);
            self.draw_pixel(x0 - y, y0 + x, color);
            self.draw_pixel(x0 - x, y0 + y, color);
            self.draw_pixel(x0 - x, y0 - y, color);
            self.draw_pixel(x0 - y, y0 - x, color);
            self.draw_pixel(x0 + y, y0 - x, color);
            self.draw_pixel(x0 + x, y0 - y, color);

            if err <= 0 {
                y += 1;
                err += 2 * y + 1;
            }
            if err > 0 {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
    }

    fn clear(&mut self, color: u32) {
        let (w, h) = self.mode.size();
        let w = w as usize;
        let h = h as usize;

        for y in 0..h {
            for x in 0..w {
                let offset = y * w + x;
                self.map[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }

    fn get_screen_size(&self) -> (u32, u32) {
        (self.mode.size().0 as u32, self.mode.size().1 as u32)
    }

    fn cleanup(&mut self) {}
}
