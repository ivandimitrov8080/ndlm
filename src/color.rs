use std::str::FromStr;

#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub opacity: f32,
}

const fn rgb(red: f32, green: f32, blue: f32) -> Color {
    Color {
        red,
        green,
        blue,
        opacity: 1.0,
    }
}

impl FromStr for Color {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("0x").unwrap();
        match u32::from_str_radix(hex, 16) {
            Ok(value) => {
                let red = ((value >> 16) & 0xFF) as f32 / 255.0;
                let green = ((value >> 8) & 0xFF) as f32 / 255.0;
                let blue = (value & 0xFF) as f32 / 255.0;
                Ok(rgb(red, green, blue))
            }
            Err(_) => Ok(rgb(255f32, 0f32, 0f32)),
        }
    }
}
impl Color {
    pub const WHITE: Self = rgb(1.0, 1.0, 1.0);
    pub const YELLOW: Self = rgb(0.75, 0.75, 0.25);
}
