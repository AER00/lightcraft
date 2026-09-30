use crate::{RawError, RawImage, Result};

pub(crate) fn decode(_bytes: &[u8]) -> Result<RawImage> {
    Err(RawError::Unsupported("nef: not yet".into()))
}
