use thiserror::Error;

use crate::draw;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
