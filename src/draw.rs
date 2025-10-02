use crate::buffer::Buffer;
use crate::color::Color;
use crate::error::Error;
use std::collections::HashMap;
use std::str::FromStr;

use lazy_static::lazy_static;
use rusttype::{point, Font as RustFont, Scale};
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
    glyphs: HashMap<char, CachedGlyph>,
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

impl Font {
    pub fn new(font: &'static RustFont<'_>, size: f32) -> Font {
        Font {
            glyphs: HashMap::new(),
            font,
            size,
        }
    }
}

#[derive(Clone)]
struct CachedGlyph {
    dimensions: (u32, u32),
    origin: (i32, i32),
    render: Vec<f32>,
}
