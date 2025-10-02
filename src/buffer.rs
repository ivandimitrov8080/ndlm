use crate::color::Color;

pub type Vect = (u32, u32);
pub type Rect = (u32, u32, u32, u32);

pub struct Buffer<'a> {
    buf: &'a mut [u8],
    dimensions: Vect,
    subdimensions: Option<Rect>,
}

impl<'a> Buffer<'a> {
    pub fn new(buf: &'a mut [u8], dimensions: Vect) -> Self {
        Self {
            buf,
            dimensions,
            subdimensions: None,
        }
    }

    pub fn memset(&mut self, c: &Color) {
        if let Some(subdim) = self.subdimensions {
            unsafe {
                let ptr = self.buf.as_mut_ptr();
                for y in subdim.1..(subdim.1 + subdim.3) {
                    for x in subdim.0..(subdim.0 + subdim.2) {
                        *((ptr as *mut u32).offset((x + y * self.dimensions.0) as isize)) =
                            c.as_argb8888();
                    }
                }
            }
        } else {
            unsafe {
                let ptr = self.buf.as_mut_ptr();
                for p in 0..(self.dimensions.0 * self.dimensions.1) {
                    *((ptr as *mut u32).offset(p as isize)) = c.as_argb8888();
                }
            }
        }
    }

    pub fn put(&mut self, pos: Vect, c: &Color) -> Result<(), ()> {
        let true_pos = if let Some(subdim) = self.subdimensions {
            if pos.0 >= subdim.2 || pos.1 >= subdim.3 {
                return Err(());
            }
            (pos.0 + subdim.0, pos.1 + subdim.1)
        } else {
            if pos.0 >= self.dimensions.0 || pos.1 >= self.dimensions.1 {
                return Err(());
            }
            pos
        };

        unsafe {
            let ptr = self
                .buf
                .as_mut_ptr()
                .offset(4 * (true_pos.0 + (true_pos.1 * self.dimensions.0)) as isize);
            *(ptr as *mut u32) = c.as_argb8888();
        };

        Ok(())
    }
}
