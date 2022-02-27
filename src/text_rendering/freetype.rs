use std::convert::TryFrom;
use std::sync::Arc;

pub struct FtLibrary {
    inner: freetype::freetype::FT_Library,
}

impl FtLibrary {
    pub fn new() -> Result<FtLibrary, FtError> {
        let mut ft: freetype::freetype::FT_Library = std::ptr::null_mut();
        FtError::from_ft(unsafe { freetype::freetype::FT_Init_FreeType(&mut ft) })?;
        Ok(FtLibrary { inner: ft })
    }

    pub(super) fn as_raw(&self) -> freetype::freetype::FT_Library {
        self.inner
    }
}

impl Drop for FtLibrary {
    fn drop(&mut self) {
        let result = FtError::from_ft(unsafe { freetype::freetype::FT_Done_FreeType(self.inner) });
        result.unwrap();
    }
}

pub struct FtFace {
    library: Arc<FtLibrary>,
    buffer: Option<Box<[u8]>>,
    face: freetype::freetype::FT_Face,
}

impl FtFace {
    pub fn new_from_buffer(library: Arc<FtLibrary>, buffer: Box<[u8]>) -> Result<FtFace, FtError> {
        let mut face: freetype::freetype::FT_Face = std::ptr::null_mut();
        let open_args = freetype::freetype::FT_Open_Args {
            flags: freetype::freetype::FT_OPEN_MEMORY,
            memory_base: buffer.as_ptr(),
            memory_size: freetype::freetype::FT_Long::try_from(buffer.len()).map_err(|_| FtError::FaceOpenMemoryBadLen)?,
            pathname: std::ptr::null_mut(),
            stream: std::ptr::null_mut(),
            driver: std::ptr::null_mut(),
            num_params: 0,
            params: std::ptr::null_mut(),
        };
        let raw_lib = library.as_raw();
        let error = unsafe {
            freetype::freetype::FT_Open_Face(
                raw_lib,
                &open_args,
                0,
                &mut face,
            )
        };
        FtError::from_ft(error)?;
        Ok(FtFace {
            library,
            buffer: Some(buffer),
            face,
        })
    }

    pub fn library(&self) -> &Arc<FtLibrary> {
        &self.library
    }

    pub(super) fn as_raw(&self) -> freetype::freetype::FT_Face {
        self.face
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FtError {
    #[error("FreeType error: {0}")]
    Freetype(freetype::freetype::FT_Error),
    #[error("while opening a face, the length could not be converted to an FT_Long")]
    FaceOpenMemoryBadLen,
}

impl FtError {
    pub(super) fn from_ft(outer: freetype::freetype::FT_Error) -> Result<(), FtError> {
        match outer {
            0 => Ok(()),
            n => Err(FtError::Freetype(n)),
        }
    }
}