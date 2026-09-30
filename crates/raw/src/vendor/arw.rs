use crate::{RawError, RawImage, Result};

pub(crate) fn decode(_bytes: &[u8]) -> Result<RawImage> {
    Err(RawError::Unsupported("arw: not yet".into()))
}
