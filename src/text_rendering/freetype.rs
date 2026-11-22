use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

pub struct FtLibrary {
    inner: freetype::freetype::FT_Library,
}

impl FtLibrary {
    pub fn new() -> Result<FtLibrary, FtError> {
        let mut ft: freetype::freetype::FT_Library = std::ptr::null_mut();
        FtError::from_ft(unsafe { freetype::freetype::FT_Init_FreeType(&mut ft) })?;
        Ok(FtLibrary { inner: ft })
    }

    pub(super) fn as_mut_raw(&mut self) -> freetype::freetype::FT_Library {
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
    library: Arc<Mutex<FtLibrary>>,
    _buffer: Option<Box<[u8]>>,
    face: freetype::freetype::FT_Face,
}

impl FtFace {
    pub fn new_from_buffer(
        library: Arc<Mutex<FtLibrary>>,
        buffer: Box<[u8]>,
    ) -> Result<FtFace, FtError> {
        let mut face: freetype::freetype::FT_Face = std::ptr::null_mut();
        let open_args = freetype::freetype::FT_Open_Args {
            flags: freetype::freetype::FT_OPEN_MEMORY,
            memory_base: buffer.as_ptr(),
            memory_size: freetype::freetype::FT_Long::try_from(buffer.len())
                .map_err(|_| FtError::FaceOpenMemoryBadLen)?,
            pathname: std::ptr::null_mut(),
            stream: std::ptr::null_mut(),
            driver: std::ptr::null_mut(),
            num_params: 0,
            params: std::ptr::null_mut(),
        };
        {
            let mut library_lock = library.lock().unwrap();
            let raw_lib = library_lock.as_mut_raw();
            let error =
                unsafe { freetype::freetype::FT_Open_Face(raw_lib, &open_args, 0, &mut face) };
            FtError::from_ft(error)?;
        }
        Ok(FtFace {
            library,
            _buffer: Some(buffer),
            face,
        })
    }

    pub fn library(&self) -> &Arc<Mutex<FtLibrary>> {
        &self.library
    }

    pub fn set_char_size(
        &mut self,
        char_height: freetype::freetype::FT_F26Dot6,
    ) -> Result<(), FtError> {
        let err = unsafe { freetype::freetype::FT_Set_Char_Size(self.face, 0, char_height, 0, 0) };
        FtError::from_ft(err)
    }

    pub(super) fn as_mut_raw(&mut self) -> freetype::freetype::FT_Face {
        self.face
    }

    #[warn(unsafe_op_in_unsafe_fn)]
    pub(super) unsafe fn outline(&mut self) -> &mut freetype::freetype::FT_Outline {
        let glyph_ptr = unsafe { *(self.face as freetype::freetype::FT_Face) }.glyph;
        let glyph = unsafe { glyph_ptr.as_mut() };
        let glyph = glyph.expect("Face glyph slot was null during call to outline()");
        if glyph.format != freetype::freetype::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE {
            panic!("Loaded glyph was not an outline.");
        }
        &mut glyph.outline
    }

    /*
    pub unsafe fn render_current_glyph_outline<T>(&mut self, user_func: fn() -> Result<(), FtError> {
        if unsafe { *(*raw_face).glyph }.format != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE {
            return Err(FtError::GlyphIsNotOutline);
        }
    }
    */
}

impl Drop for FtFace {
    fn drop(&mut self) {
        let result = FtError::from_ft(unsafe { freetype::freetype::FT_Done_Face(self.face) });
        result.unwrap();
    }
}

/*
extern "C" fn render_callback(y: std::os::raw::c_int, count: std::os::raw::c_int, spans: *const ft_lib::FT_Span, user: *mut std::os::raw::c_void) {
    let spans = unsafe {
        let slice = std::ptr::slice_from_raw_parts(spans, count);
        &*slice
    };
}
*/

#[derive(Debug, thiserror::Error)]
pub enum FtError {
    #[error("FreeType error: {0}")]
    Freetype(freetype::freetype::FT_Error),
    #[error("while opening a face, the length could not be converted to an FT_Long")]
    FaceOpenMemoryBadLen,
    #[error("the selected glyph is not an outline")]
    GlyphIsNotOutline,
}

impl FtError {
    pub(super) fn from_ft(outer: freetype::freetype::FT_Error) -> Result<(), FtError> {
        match outer {
            0 => Ok(()),
            n => Err(FtError::Freetype(n)),
        }
    }
}
