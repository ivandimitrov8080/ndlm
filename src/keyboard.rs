// Keyboard input abstraction for both framebuffer (stdin) and DRM (evdev)

#[derive(Debug)]
pub enum KeyEvent {
    Char(char),
    // Add more variants as needed (e.g., special keys)
}

pub trait KeyboardInput {
    fn next_key_event(&mut self) -> Option<KeyEvent>;
}

#[cfg(feature = "framebuffer")]
pub mod fb_input {
    use super::{KeyEvent, KeyboardInput};
    use std::io::{self, Read};

    pub struct StdinInput<R: Read> {
        bytes: io::Bytes<R>,
    }

    impl StdinInput<std::io::StdinLock<'static>> {
        pub fn new() -> Self {
            // Safety: 'static is fine for stdin
            let stdin = Box::leak(Box::new(std::io::stdin()));
            Self {
                bytes: stdin.lock().bytes(),
            }
        }
    }

    impl<R: Read> KeyboardInput for StdinInput<R> {
        fn next_key_event(&mut self) -> Option<KeyEvent> {
            while let Some(Ok(byte)) = self.bytes.next() {
                if let Some(ch) = char::from_u32(byte as u32) {
                    return Some(KeyEvent::Char(ch));
                }
            }
            None
        }
    }
}

#[cfg(feature = "drm")]
pub mod drm_input {
    use super::{KeyEvent, KeyboardInput};
    use evdev::{Device, InputEventKind};

    pub struct EvdevInput {
        device: Device,
    }

    impl EvdevInput {
        pub fn new(path: &str) -> std::io::Result<Self> {
            let device = Device::open(path)?;
            Ok(Self { device })
        }
    }

    impl KeyboardInput for EvdevInput {
        fn next_key_event(&mut self) -> Option<KeyEvent> {
            if let Ok(events) = self.device.fetch_events() {
                for ev in events {
                    if let InputEventKind::Key(_key) = ev.kind() {
                        // For demo: just return a dummy char for any key event
                        // In real code, map keycode to char or enum
                        return Some(KeyEvent::Char('?'));
                    }
                }
            }
            None
        }
    }
}
