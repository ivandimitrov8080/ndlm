use crate::error::Error;

use std::str::FromStr;

use lazy_static::lazy_static;
use rusttype::Font as RustFont;
use thiserror::Error as ThisError;

pub static DEJAVUSANS_MONO_FONT_DATA: &[u8] = include_bytes!("../fonts/dejavu/DejaVuSansMono.ttf");

lazy_static! {
    pub static ref DEJAVUSANS_MONO: RustFont<'static> =
        RustFont::try_from_bytes(DEJAVUSANS_MONO_FONT_DATA as &[u8])
            .expect("error constructing DejaVuSansMono");
}

#[derive(ThisError, Debug)]
#[non_exhaustive]
pub enum DrawError {
    #[error("glyph for {0} not in cache")]
    GlyphNotInCache(char),
}

#[derive(Clone)]
pub struct Font {
    font: &'static RustFont<'static>,
    size: f32,
}

impl Default for Font {
    fn default() -> Self {
        Font::new(&DEJAVUSANS_MONO, 72.0)
    }
}

impl FromStr for Font {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let size = s.split(" ").last().unwrap().parse().unwrap();
        Ok(Font::new(&DEJAVUSANS_MONO, size))
    }
}

use crate::p5::P5;
use rusttype::{point, Scale};

impl Font {
    pub fn new(font: &'static RustFont<'_>, size: f32) -> Font {
        Font { font, size }
    }

    pub fn draw_text(&self, p5: &mut P5, x: i32, y: i32, text: &str, color: u32) {
        let scale = Scale::uniform(self.size);
        let v_metrics = self.font.v_metrics(scale);
        let start = point(x as f32, y as f32 + v_metrics.ascent);
        let glyphs: Vec<_> = self.font.layout(text, scale, start).collect();

        for g in glyphs {
            if let Some(bb) = g.pixel_bounding_box() {
                g.draw(|gx, gy, gv| {
                    if gv > 0.0 {
                        let gx = gx as i32 + bb.min.x;
                        let gy = gy as i32 + bb.min.y;
                        p5.rect(gx, gy, gx + 1, gy + 1, color);
                    }
                });
            }
        }
    }
}
