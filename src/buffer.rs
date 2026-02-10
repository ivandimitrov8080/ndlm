use thiserror::Error;

use crate::color::Color;

pub type Vect = (u32, u32);
pub type Rect = (u32, u32, u32, u32);

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BufferError {
    #[error("cannot create subdimensions larger than buffer: {subdimensions:?} > {bounds:?}")]
    SubdimensionsTooLarge { subdimensions: Rect, bounds: Rect },
    #[error("cannot create offset outside buffer: {offset:?} > {bounds:?}")]
    OffsetOutOfBounds { offset: Vect, bounds: Rect },
    #[error("put({pos:?}) is not within subdimensions of buffer ({subdim:?})")]
    PixelOutOfSubdimBounds { pos: Vect, subdim: Rect },
    #[error("put({pos:?}) is not within dimensions of buffer ({dim:?})")]
    PixelOutOfBounds { pos: Vect, dim: Vect },
}

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
}
