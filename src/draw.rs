use crate::color::Color;
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DrawError {
    #[error("glyph for {0} not in cache")]
    GlyphNotInCache(char),
}

use cairo::{Context as CairoContext, Format, ImageSurface};
use pango::FontDescription;
use pangocairo::functions::{create_layout, show_layout};

pub fn draw_text(
    framebuffer: &mut [u8],
    dimensions: (u32, u32),
    x: i32,
    y: i32,
    text: &str,
    font: &str,
    color: &Color,
) {
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
    )
    .expect("Could not create cairo surface");

    let cr = CairoContext::new(&surface).unwrap();
    cr.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        color.opacity as f64,
    );

    let layout = create_layout(&cr);
    layout.set_text(text);
    let font_desc = FontDescription::from_string(font);
    layout.set_font_description(Some(&font_desc));
    cr.move_to(x as f64, y as f64);
    show_layout(&cr, &layout);
}

pub fn fill_rect(
    framebuffer: &mut [u8],
    dimensions: (u32, u32),
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: &Color,
) {
    let fb_width = dimensions.0 as i32;
    let fb_height = dimensions.1 as i32;
    let stride = fb_width * 4;
    let surface = ImageSurface::create_for_data(
        unsafe {
            std::slice::from_raw_parts_mut(framebuffer.as_mut_ptr(), (stride * fb_height) as usize)
        },
        Format::ARgb32,
        fb_width,
        fb_height,
        stride,
    )
    .expect("Could not create cairo surface");

    let cr = CairoContext::new(&surface).unwrap();
    cr.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        color.opacity as f64,
    );
    cr.rectangle(x as f64, y as f64, width as f64, height as f64);
    let _ = cr.fill();
}
