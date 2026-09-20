use std::str::FromStr;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
    pub const BLACK: Self = rgb(0.0, 0.0, 0.0);

    /// Linear blend towards `other`, `t` clamped to `0..=1`.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            red: self.red + (other.red - self.red) * t,
            green: self.green + (other.green - self.green) * t,
            blue: self.blue + (other.blue - self.blue) * t,
            opacity: self.opacity + (other.opacity - self.opacity) * t,
        }
    }

    pub fn lighten(self, amount: f32) -> Self {
        self.mix(Self::WHITE, amount)
    }

    pub fn darken(self, amount: f32) -> Self {
        self.mix(Self::BLACK, amount)
    }

    pub fn with_opacity(self, opacity: f32) -> Self {
        Self { opacity, ..self }
    }

    /// Perceived brightness, used to decide whether a theme is dark or light.
    pub fn luminance(self) -> f32 {
        0.2126 * self.red + 0.7152 * self.green + 0.0722 * self.blue
    }

    /// Distance between the strongest and weakest channel, i.e. how colourful
    /// the colour is.
    pub fn chroma(self) -> f32 {
        let max = self.red.max(self.green).max(self.blue);
        let min = self.red.min(self.green).min(self.blue);
        max - min
    }
}
