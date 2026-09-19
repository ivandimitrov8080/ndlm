#![deny(rust_2018_idioms)]

use std::fs;
use std::str::FromStr;

use pango::FontDescription;
use termion::raw::IntoRawMode;
use thiserror::Error;

use crate::{color::Color, manager::LoginManager};

mod color;
mod draw;
mod greetd;
mod manager;
mod vt;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
struct Module {
    font: FontDescription,
    title_font: FontDescription,
    image_dir: String,
    dialog_horizontal_alignment: f32,
    dialog_vertical_alignment: f32,
    title_horizontal_alignment: f32,
    title_vertical_alignment: f32,
    watermark_horizontal_alignment: f32,
    watermark_vertical_alignment: f32,
    horizontal_alignment: f32,
    vertical_alignment: f32,
    background_start_color: Color,
    background_end_color: Color,
}
impl Default for Module {
    fn default() -> Self {
        Module {
            font: FontDescription::from_string(""),
            title_font: FontDescription::from_string(""),
            image_dir: "".to_string(),
            dialog_horizontal_alignment: 0f32,
            dialog_vertical_alignment: 0f32,
            title_horizontal_alignment: 0f32,
            title_vertical_alignment: 0f32,
            watermark_horizontal_alignment: 0f32,
            watermark_vertical_alignment: 0f32,
            horizontal_alignment: 0f32,
            vertical_alignment: 0f32,
            background_start_color: Color::default(),
            background_end_color: Color::default(),
        }
    }
}

impl FromStr for Module {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut module = Module::default();
        for l in s.lines() {
            if l.contains("=") {
                let (key, value) = match &l.split("=").collect::<Vec<&str>>()[..] {
                    &[first, second, ..] => (first, second),
                    _ => unreachable!(),
                };
                let mut v = 0f32;
                if value.starts_with(".") {
                    v = format!("0{}", value).parse().unwrap();
                }
                match key {
                    "Font" => module.font = FontDescription::from_string(value),
                    "TitleFont" => module.title_font = FontDescription::from_string(value),
                    "ImageDir" => module.image_dir = value.to_string(),
                    "DialogHorizontalAlignment" => module.dialog_horizontal_alignment = v,
                    "DialogVerticalAlignment" => module.dialog_vertical_alignment = v,
                    "TitleHorizontalAlignment" => module.title_horizontal_alignment = v,
                    "TitleVerticalAlignment" => module.title_vertical_alignment = v,
                    "HorizontalAlignment" => module.horizontal_alignment = v,
                    "VerticalAlignment" => module.vertical_alignment = v,
                    "WatermarkHorizontalAlignment" => module.watermark_horizontal_alignment = v,
                    "WatermarkVerticalAlignment" => module.watermark_vertical_alignment = v,
                    "BackgroundStartColor" => {
                        module.background_start_color = value.parse().unwrap()
                    }
                    "BackgroundEndColor" => module.background_end_color = value.parse().unwrap(),
                    _ => {}
                }
            }
        }
        Ok(module)
    }
}

#[derive(Default, Clone)]
struct Theme {
    name: String,
    description: Option<String>,
    module: Module,
}

impl FromStr for Theme {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut theme = Theme::default();
        for l in s.lines() {
            if l.contains("=") {
                let (key, value) = match &l.split("=").collect::<Vec<&str>>()[..] {
                    &[first, second, ..] => (first, second),
                    _ => unreachable!(),
                };
                match key {
                    "Name" => theme.name = value.to_string(),
                    "Description" => theme.description = Some(value.to_string()),
                    "ModuleName" => theme.module = s.parse().unwrap(),
                    _ => {}
                }
            }
        }
        Ok(theme)
    }
}

#[derive(Default, Clone)]
struct Config {
    session: Vec<String>,
    theme: Theme,
}

fn parse_theme(theme_file: String) -> Theme {
    let content = fs::read_to_string(theme_file).expect("Unable to read theme file");
    content.parse().unwrap()
}

fn parse_args() -> Config {
    let mut args = std::env::args().skip(1); // skip program name
    let mut config = Config::default();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => {
                if let Some(value) = args.next() {
                    config.session = value.split(" ").map(|s| s.to_string()).collect();
                } else {
                    eprintln!("Expected a value after --session");
                }
            }
            "--theme-file" => {
                if let Some(value) = args.next() {
                    config.theme = parse_theme(value);
                } else {
                    eprintln!("Expected a value after --theme-file");
                }
            }
            _ if arg.starts_with("--") => {
                eprintln!("Unknown flag: {}", arg);
            }
            _ => {
                println!("unknown arg {arg}");
            }
        }
    }

    config
}

fn main() {
    // DRM device opening
    use drm::Device;
    use drm::control::Device as ControlDevice;

    // Open DRM device

    // AUTO-DETECT /dev/dri/card{x}
    let drm_file = {
        use std::fs;
        let dri_dir = "/dev/dri";
        let mut card_path = None;
        if let Ok(entries) = fs::read_dir(dri_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                if fname.to_string_lossy().starts_with("card") {
                    let fullpath = entry.path();
                    if let Ok(file) = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&fullpath)
                    {
                        card_path = Some(file);
                        break;
                    }
                }
            }
        }
        card_path.expect("unable to auto-detect or open any DRM card device in /dev/dri")
    };

    let card = crate::manager::Card(drm_file);

    // Take DRM master. Opening the primary node normally grants it already, but
    // a leftover master (for example a crashed compositor) is worth reporting
    // here instead of showing up as failing page flips later on.
    if let Err(e) = card.acquire_master_lock() {
        eprintln!("unable to acquire DRM master: {e}");
    }

    // Get available connectors/modes (find connected display)
    let res_handles = card
        .resource_handles()
        .expect("Failed to get DRM resources");
    let connector_handle = res_handles.connectors()[0]; // TODO: enumerate for active
    let connector_info = card
        .get_connector(connector_handle, false)
        .expect("Failed to get connector info");
    let crtc_handle = res_handles.crtcs()[0];
    // The greeter sets the CRTC mode itself (and again after every VT switch),
    // so pick the mode the display prefers rather than whatever comes first.
    let modes = connector_info.modes();
    let mode = modes
        .iter()
        .find(|mode| {
            mode.mode_type()
                .contains(drm::control::ModeTypeFlags::PREFERRED)
        })
        .or_else(|| modes.first())
        .copied()
        .expect("connector reports no modes");

    let (width, height) = (mode.size().0 as u32, mode.size().1 as u32);

    // Allocate DumbBuffer
    use drm_fourcc::DrmFourcc;

    let dbuf = card
        .create_dumb_buffer((width, height), DrmFourcc::Xrgb8888, 32)
        .expect("Failed to allocate dumb buffer");
    let fb = card
        .add_framebuffer(&dbuf, 24, 32)
        .expect("Failed to add framebuffer");

    // Map DumbBuffer to memory
    let mut dbuf_mut = dbuf;
    let map_result = card.map_dumb_buffer(&mut dbuf_mut);
    let mut map = map_result.expect("Failed to map dumb buffer");

    let raw = std::io::stdout()
        .into_raw_mode()
        .expect("unable to enter raw mode");

    // ndlm renders on the VT greetd started it on, and only while that VT is
    // the active one.
    let vt = vt::Vt::detect();
    if !vt.is_managed() {
        eprintln!(
            "ndlm: not running on a virtual terminal, so the display cannot be \
             handed back on VT switches. Set terminal.vt in greetd's \
             configuration."
        );
    }

    // Pass mapped buffer, device, and screen size to LoginManager
    LoginManager::new(
        &mut map,
        parse_args(),
        width,
        height,
        manager::Display {
            card: &card,
            framebuffer: fb.into(),
            crtc: crtc_handle.into(),
            connector: connector_handle,
            mode,
        },
        vt,
    )
    .start();
    drop(raw);
}
