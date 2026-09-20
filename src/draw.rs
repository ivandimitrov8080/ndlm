use crate::color::Color;
use cairo::{Context as CairoContext, Format, ImageSurface, LinearGradient};
use pango::FontDescription;
use pangocairo::functions::{create_layout, show_layout};
use std::f64::consts::PI;
use thiserror::Error;

const CARD_WIDTH: f64 = 440.0;
const PADDING: f64 = 28.0;
const FIELD_HEIGHT: f64 = 44.0;
const FIELD_GAP: f64 = 12.0;
const FIELD_INSET: f64 = 20.0;
const CARD_RADIUS: f64 = 16.0;
const FIELD_RADIUS: f64 = 10.0;

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
    // Double buffer for the prompt: the whole widget is composed off-screen and
    // blitted to the scanout buffer in one go, so it never shows up half drawn.
    region_surface: Option<ImageSurface>,
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

    fn set_source(context: &CairoContext, color: &Color) {
        context.set_source_rgba(
            color.red as f64,
            color.green as f64,
            color.blue as f64,
            color.opacity as f64,
        );
    }

    /// Adds a closed sub-path describing a rectangle with rounded corners.
    fn rounded_rect(context: &CairoContext, x: f64, y: f64, width: f64, height: f64, radius: f64) {
        let radius = radius.min(width / 2.0).min(height / 2.0);
        context.new_sub_path();
        context.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
        context.arc(x + width - radius, y + radius, radius, 1.5 * PI, 2.0 * PI);
        context.arc(
            x + width - radius,
            y + height - radius,
            radius,
            0.0,
            0.5 * PI,
        );
        context.arc(x + radius, y + height - radius, radius, 0.5 * PI, PI);
        context.close_path();
    }

    /// Fills the whole surface with a vertical gradient, used for the wallpaper.
    pub fn fill_vertical_gradient(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        top: &Color,
        bottom: &Color,
    ) {
        let (x, y, width, height) = (x as f64, y as f64, width as f64, height as f64);
        let gradient: LinearGradient = LinearGradient::new(x, y, x, y + height);
        gradient.add_color_stop_rgba(
            0.0,
            top.red as f64,
            top.green as f64,
            top.blue as f64,
            top.opacity as f64,
        );
        gradient.add_color_stop_rgba(
            1.0,
            bottom.red as f64,
            bottom.green as f64,
            bottom.blue as f64,
            bottom.opacity as f64,
        );
        let _ = self.context.set_source(&gradient);
        self.context.rectangle(x, y, width, height);
        let _ = self.context.fill();
    }

    /// Starts a new off-screen region the prompt is composed in. Coordinates of
    /// everything drawn until [`Self::composite_region_to_fb`] are relative to
    /// the region's top-left corner.
    pub fn begin_region(&mut self, x: i32, y: i32, width: i32, height: i32) {
        let region_surf =
            ImageSurface::create(Format::ARgb32, width, height).expect("failed region surf");
        let region_ctx = CairoContext::new(&region_surf).unwrap();
        self.region_surface = Some(region_surf);
        self.region_context = Some(region_ctx);
        self.region_dimensions = Some((x, y, width, height));
    }

    pub fn fill_region_rounded(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        color: &Color,
    ) {
        if let Some(ctx) = self.region_context.as_ref() {
            Self::set_source(ctx, color);
            Self::rounded_rect(ctx, x, y, width, height, radius);
            let _ = ctx.fill();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_region_rounded(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        line_width: f64,
        color: &Color,
    ) {
        if let Some(ctx) = self.region_context.as_ref() {
            Self::set_source(ctx, color);
            Self::rounded_rect(ctx, x, y, width, height, radius);
            ctx.set_line_width(line_width);
            let _ = ctx.stroke();
        }
    }

    pub fn fill_region_rect(&mut self, x: f64, y: f64, width: f64, height: f64, color: &Color) {
        if let Some(ctx) = self.region_context.as_ref() {
            Self::set_source(ctx, color);
            ctx.rectangle(x, y, width, height);
            let _ = ctx.fill();
        }
    }

    /// Measures a single line of text, without drawing it.
    pub fn text_size(&self, text: &str, font: &FontDescription) -> (f64, f64) {
        let context = self.region_context.as_ref().unwrap_or(&self.context);
        let layout = create_layout(context);
        layout.set_text(text);
        layout.set_font_description(Some(font));
        let (width, height) = layout.pixel_size();
        (width as f64, height as f64)
    }

    /// Draws a single line of text into the region, with `(x, y)` being the
    /// top-left corner of the line box. Returns the size of the drawn text.
    pub fn draw_text_region(
        &mut self,
        text: &str,
        font: &FontDescription,
        color: &Color,
        x: f64,
        y: f64,
    ) -> (f64, f64) {
        let mut size = (0.0, 0.0);
        if let Some(ctx) = self.region_context.as_ref() {
            Self::set_source(ctx, color);
            let layout = create_layout(ctx);
            layout.set_text(text);
            layout.set_font_description(Some(font));
            let (width, height) = layout.pixel_size();
            size = (width as f64, height as f64);
            ctx.move_to(x, y);
            show_layout(ctx, &layout);
        }
        size
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

/// Which input field has the focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Username,
    Password,
}

/// Colours and fonts the prompt is drawn with.
pub struct PromptStyle {
    /// Main text.
    pub primary: Color,
    /// Labels and other secondary text.
    pub secondary: Color,
    /// The prompt panel.
    pub card: Color,
    /// The input fields inside the panel.
    pub field: Color,
    /// Unfocused outlines.
    pub border: Color,
    /// Highlight for the focused field and other accents.
    pub accent: Color,
    pub title_font: FontDescription,
    pub value_font: FontDescription,
    pub label_font: FontDescription,
    pub hint_font: FontDescription,
}

/// Resolves the theme's background colours, dropping the alpha (an unset colour
/// comes through as fully transparent) and treating a missing gradient end as a
/// flat background.
pub fn background_colors(start: Color, end: Color) -> (Color, Color) {
    let start = start.with_opacity(1.0);
    let end = if end.opacity == 0.0 {
        start
    } else {
        end.with_opacity(1.0)
    };
    (start, end)
}

impl PromptStyle {
    /// Derives a palette from the Plymouth theme's background gradient, so the
    /// prompt picks up whatever colours the boot splash was using.
    pub fn from_backgrounds(start: Color, end: Color) -> Self {
        let (start, end) = background_colors(start, end);
        let base = start.mix(end, 0.5);
        let dark = base.luminance() < 0.5;

        let primary = if dark {
            base.mix(Color::WHITE, 0.94)
        } else {
            base.mix(Color::BLACK, 0.92)
        };
        let secondary = primary.mix(base, 0.45);
        let card = if dark {
            base.lighten(0.08)
        } else {
            base.darken(0.05)
        };
        let field = if dark {
            base.lighten(0.03)
        } else {
            base.mix(Color::WHITE, 0.6)
        };
        let border = if dark {
            base.lighten(0.16)
        } else {
            base.darken(0.12)
        };

        // The accent is the gradient's colour with its saturation exaggerated,
        // so it stands apart from the greys the panel is built from. Themes
        // without any colour at all fall back to plain text.
        let mid = start.mix(end, 0.65);
        let level = mid.luminance();
        let boost = 1.8;
        let saturated = Color {
            red: (level + (mid.red - level) * boost).clamp(0.0, 1.0),
            green: (level + (mid.green - level) * boost).clamp(0.0, 1.0),
            blue: (level + (mid.blue - level) * boost).clamp(0.0, 1.0),
            opacity: 1.0,
        };
        let accent = if saturated.chroma() < 0.06 {
            primary
        } else if dark {
            saturated.lighten(0.3)
        } else {
            saturated.darken(0.2)
        };

        Self {
            primary,
            secondary,
            card,
            field,
            border,
            accent,
            title_font: FontDescription::from_string("DejaVu Sans Mono Bold 20"),
            value_font: FontDescription::from_string("DejaVu Sans Mono 17"),
            label_font: FontDescription::from_string("DejaVu Sans Mono 13"),
            hint_font: FontDescription::from_string("DejaVu Sans Mono 13"),
        }
    }
}

/// Everything drawn on one frame.
pub struct Prompt<'a> {
    pub title: &'a str,
    pub username: &'a str,
    /// Drawn as-is, masking is up to the caller.
    pub password: &'a str,
    /// The selected session, `None` when there is nothing to choose from.
    pub session: Option<&'a str>,
    pub focused: Field,
    /// Point the theme's dialog alignment points at.
    pub center: (u32, u32),
    /// Size of the display the prompt is centered and clamped on.
    pub screen: (u32, u32),
}

/// Draws the login prompt centred on `prompt.center`.
pub fn draw_prompt(surf: &mut FramebufferSurface, prompt: &Prompt<'_>, style: &PromptStyle) {
    // Measure everything first so the card can be sized and centred around the
    // alignment point the theme picked.
    let title_size = surf.text_size(prompt.title, &style.title_font);
    let line_height = surf.text_size("M", &style.value_font).1.max(1.0);
    let hint_block = surf.text_size("M", &style.hint_font).1.max(16.0);
    let title_block = title_size.1.max(28.0);
    let field_width = CARD_WIDTH - FIELD_INSET * 2.0;

    let username_y = PADDING + title_block + 16.0;
    let password_y = username_y + FIELD_HEIGHT + FIELD_GAP;
    let hint_y = password_y + FIELD_HEIGHT + 20.0;
    let card_height = hint_y + hint_block + PADDING;

    // Keep the panel on screen even if the theme points the dialog somewhere
    // silly.
    let max_x = (prompt.screen.0 as f64 - CARD_WIDTH).max(0.0);
    let max_y = (prompt.screen.1 as f64 - card_height).max(0.0);
    let card_x = (prompt.center.0 as f64 - CARD_WIDTH / 2.0).clamp(0.0, max_x);
    let card_y = (prompt.center.1 as f64 - card_height / 2.0).clamp(0.0, max_y);

    surf.begin_region(
        card_x as i32,
        card_y as i32,
        CARD_WIDTH as i32,
        card_height as i32,
    );

    // Panel, with a hairline so it reads as a surface even when the theme's
    // background is nearly flat.
    surf.fill_region_rounded(
        0.5,
        0.5,
        CARD_WIDTH - 1.0,
        card_height - 1.0,
        CARD_RADIUS,
        &style.card,
    );
    surf.stroke_region_rounded(
        0.5,
        0.5,
        CARD_WIDTH - 1.0,
        card_height - 1.0,
        CARD_RADIUS,
        1.0,
        &style.border,
    );

    if !prompt.title.is_empty() {
        surf.draw_text_region(
            prompt.title,
            &style.title_font,
            &style.primary,
            (CARD_WIDTH - title_size.0) / 2.0,
            PADDING + (title_block - title_size.1) / 2.0,
        );
    }

    draw_field(
        surf,
        FIELD_INSET,
        username_y,
        field_width,
        "Username",
        prompt.username,
        prompt.focused == Field::Username,
        line_height,
        style,
    );
    draw_field(
        surf,
        FIELD_INSET,
        password_y,
        field_width,
        "Password",
        prompt.password,
        prompt.focused == Field::Password,
        line_height,
        style,
    );

    if let Some(session) = prompt.session {
        let hint_size = surf.text_size(session, &style.hint_font);
        surf.draw_text_region(
            session,
            &style.hint_font,
            &style.secondary,
            (CARD_WIDTH - hint_size.0) / 2.0,
            hint_y + (hint_block - hint_size.1) / 2.0,
        );
    }

    surf.composite_region_to_fb();
}

/// Draws one input field: a rounded box with a dim label and the value next to
/// it. The focused field gets an accent outline, brighter text and a caret.
#[allow(clippy::too_many_arguments)]
fn draw_field(
    surf: &mut FramebufferSurface,
    x: f64,
    y: f64,
    width: f64,
    label: &str,
    value: &str,
    focused: bool,
    line_height: f64,
    style: &PromptStyle,
) {
    let (outline, text) = if focused {
        (style.accent, style.primary)
    } else {
        (style.border, style.secondary)
    };

    surf.fill_region_rounded(x, y, width, FIELD_HEIGHT, FIELD_RADIUS, &style.field);
    surf.stroke_region_rounded(
        x + 0.5,
        y + 0.5,
        width - 1.0,
        FIELD_HEIGHT - 1.0,
        FIELD_RADIUS,
        if focused { 2.0 } else { 1.0 },
        &outline,
    );

    // Line heights come from a reference glyph so that the text stays put when
    // the value is empty.
    let label_height = surf.text_size("M", &style.label_font).1.max(1.0);
    let label_x = x + 16.0;
    let label_width = surf.text_size(label, &style.label_font).0;
    surf.draw_text_region(
        label,
        &style.label_font,
        &style.secondary,
        label_x,
        y + (FIELD_HEIGHT - label_height) / 2.0,
    );

    let value_x = label_x + label_width + 14.0;
    let value_width = surf.text_size(value, &style.value_font).0;
    surf.draw_text_region(
        value,
        &style.value_font,
        &text,
        value_x,
        y + (FIELD_HEIGHT - line_height) / 2.0,
    );

    if focused {
        let caret_height = line_height * 0.9;
        surf.fill_region_rect(
            value_x + value_width + 2.0,
            y + (FIELD_HEIGHT - caret_height) / 2.0,
            2.0,
            caret_height,
            &style.accent,
        );
    }
}
