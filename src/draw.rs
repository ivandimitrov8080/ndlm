use crate::color::Color;
use cairo::{Context as CairoContext, Format, ImageSurface};
use pango::FontDescription;
use pangocairo::functions::{create_layout, show_layout};
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DrawError {
    #[error("glyph for {0} not in cache")]
    GlyphNotInCache(char),
    #[error("Cairo error: {0}")]
    Cairo(#[from] cairo::Error),
}

pub struct FramebufferSurface {
    context: CairoContext,
}

impl FramebufferSurface {
    pub fn new(framebuffer: &mut [u8], dimensions: (u32, u32)) -> Result<Self, DrawError> {
        let width = dimensions.0 as i32;
        let height = dimensions.1 as i32;
        let stride = width * 4;
        let surface = ImageSurface::create_for_data(
            unsafe {
                std::slice::from_raw_parts_mut(framebuffer.as_mut_ptr(), (stride * height) as usize)
            },
            Format::ARgb32,
            width,
            height,
            stride,
        )?;
        let context = CairoContext::new(&surface).unwrap();
        Ok(Self { context })
    }

    pub fn fill_rect(&self, x: i32, y: i32, width: i32, height: i32, color: &Color) {
        self.context.set_source_rgba(
            color.red as f64,
            color.green as f64,
            color.blue as f64,
            color.opacity as f64,
        );
        self.context
            .rectangle(x as f64, y as f64, width as f64, height as f64);
        let _ = self.context.fill();
    }

    pub fn draw_text(&self, x: i32, y: i32, text: &str, font: &str, color: &Color) {
        self.context.set_source_rgba(
            color.red as f64,
            color.green as f64,
            color.blue as f64,
            color.opacity as f64,
        );
        let layout = create_layout(&self.context);
        layout.set_text(text);
        let font_desc = FontDescription::from_string(font);
        layout.set_font_description(Some(&font_desc));
        self.context.move_to(x as f64, y as f64);
        show_layout(&self.context, &layout);
    }
}
