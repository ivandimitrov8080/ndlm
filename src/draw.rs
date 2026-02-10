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
    // Double buffer for input region
    region_surface: Option<ImageSurface>, // input field region
    region_context: Option<CairoContext>,
    region_dimensions: Option<(i32, i32, i32, i32)>, // x, y, width, height
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
        Ok(Self {
            context,
            region_surface: None,
            region_context: None,
            region_dimensions: None,
        })
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

    // Region fill for input fields
    pub fn fill_input_region(&mut self, x: i32, y: i32, width: i32, height: i32, color: &Color) {
        // Create region surface/context if not already
        // Always clear input region before drawing new frame
        let region_surf =
            ImageSurface::create(Format::ARgb32, width, height).expect("failed region surf");
        let region_ctx = CairoContext::new(&region_surf).unwrap();
        self.region_surface = Some(region_surf);
        self.region_context = Some(region_ctx);
        self.region_dimensions = Some((x, y, width, height));
        let ctx = self.region_context.as_mut().unwrap();
        ctx.set_source_rgba(
            color.red as f64,
            color.green as f64,
            color.blue as f64,
            color.opacity as f64,
        );
        ctx.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = ctx.fill();
    }

    pub fn draw_text_region(&mut self, text: &str, font: &str, color: &Color, y_offset: i32) {
        if let Some(ctx) = self.region_context.as_mut() {
            ctx.set_source_rgba(
                color.red as f64,
                color.green as f64,
                color.blue as f64,
                color.opacity as f64,
            );
            let layout = create_layout(ctx);
            layout.set_text(text);
            let font_desc = FontDescription::from_string(font);
            layout.set_font_description(Some(&font_desc));
            ctx.move_to(0.0, y_offset as f64);
            show_layout(ctx, &layout);
        }
    }

    pub fn composite_region_to_fb(&mut self) {
        if let (Some(region_surf), Some((x, y, w, h))) =
            (self.region_surface.as_ref(), self.region_dimensions)
        {
            let _ = self
                .context
                .set_source_surface(region_surf, x as f64, y as f64);
            self.context
                .rectangle(x as f64, y as f64, w as f64, h as f64);
            let _ = self.context.fill();
        }
    }
}
