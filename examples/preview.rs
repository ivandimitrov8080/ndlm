//! Renders the login prompt into a PNG, so a Plymouth theme can be previewed
//! without booting greetd. The theme's background colours are what the prompt
//! derives its palette from, so passing them is enough to see the result:
//!
//!     cargo run --example preview -- --start 0x0b0d10 --end 0x1d2b3a
//!
//! Writes `preview.png` (override with `--out`).

#[path = "../src/color.rs"]
mod color;
#[path = "../src/draw.rs"]
mod draw;

/// `color.rs` refers to `crate::Error` in its `FromStr` implementation.
#[derive(Debug)]
pub enum Error {}

use cairo::{Format, ImageSurface};
use color::Color;
use draw::{Field, FramebufferSurface, Prompt, PromptStyle};
use std::str::FromStr;

fn main() {
    let mut start = "0x101418".to_string();
    let mut end = "0x24384a".to_string();
    let mut title = "Welcome".to_string();
    let mut out = "preview.png".to_string();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match arg.as_str() {
            "--start" => start = value(),
            "--end" => end = value(),
            "--title" => title = value(),
            "--out" => out = value(),
            other => println!("unknown arg {other}"),
        }
    }

    let start = Color::from_str(&start).expect("invalid --start colour");
    let end = Color::from_str(&end).expect("invalid --end colour");

    let screen = (1280u32, 720u32);
    let stride = (screen.0 * 4) as i32;
    let mut buffer = vec![0u8; (stride * screen.1 as i32) as usize];

    {
        let mut surface =
            FramebufferSurface::new(&mut buffer, screen).expect("could not create framebuffer");
        let (top, bottom) = draw::background_colors(start, end);
        surface.fill_vertical_gradient(0, 0, screen.0 as i32, screen.1 as i32, &top, &bottom);
        let prompt = Prompt {
            title: &title,
            username: "ivan",
            password: "••••••",
            session: Some("←  sway  →"),
            focused: Field::Password,
            center: (screen.0 / 2, screen.1 / 2),
            screen,
        };
        draw::draw_prompt(
            &mut surface,
            &prompt,
            &PromptStyle::from_backgrounds(start, end),
        );
    }

    let image = ImageSurface::create_for_data(
        buffer,
        Format::ARgb32,
        screen.0 as i32,
        screen.1 as i32,
        stride,
    )
    .expect("could not create image surface");
    let mut file = std::fs::File::create(&out).expect("could not create output file");
    image.write_to_png(&mut file).expect("could not write PNG");
    println!("wrote {out}");
}
