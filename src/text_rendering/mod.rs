use std::convert::TryFrom;

use ::freetype::freetype as ft_lib;

pub mod freetype;
mod harfbuzz;

pub fn render_text(s: &str, face: &freetype::FtFace) -> Result<Image, RenderError> {
    let raw_face = face.as_raw();
    let err = unsafe { ft_lib::FT_Set_Char_Size(raw_face, 0, 14 << 6, 0, 0) };
    freetype::FtError::from_ft(err)?;
    let hb_font = unsafe { harfbuzz::hb_ft_font_create(raw_face) };
    unsafe { harfbuzz::hb_ft_font_set_funcs(hb_font) };
    let buffer = unsafe { harfbuzz::hb_buffer_create() };
    let success = unsafe { harfbuzz::hb_buffer_allocation_successful(buffer) }.as_bool();
    if !success {
        return Err(RenderError::HarfbuzzBufferAllocFailed);
    }
    unsafe {
        harfbuzz::hb_buffer_set_direction(buffer, harfbuzz::hb_direction_t::HB_DIRECTION_LTR);
    }
    {
        let len = libc::c_int::try_from(s.as_bytes().len()).unwrap();
        unsafe {
            harfbuzz::hb_buffer_add_utf8(buffer, std::mem::transmute(s.as_bytes().as_ptr()), len, 0, len);
        }
    }
    unsafe {
        harfbuzz::hb_shape(hb_font, buffer, std::ptr::null(), 0);
    }
    let glyphs = unsafe {
        let mut len = 0;
        let glyphs = harfbuzz::hb_buffer_get_glyph_positions(buffer, &mut len);
        let slice = std::ptr::slice_from_raw_parts(glyphs, usize::try_from(len).unwrap());
        &*slice
    };
    let glyph_infos = unsafe {
        let mut len = 0;
        let glyph_infos = harfbuzz::hb_buffer_get_glyph_infos(buffer, &mut len);
        let slice = std::ptr::slice_from_raw_parts(glyph_infos, usize::try_from(len).unwrap());
        &*slice
    };
    assert!(glyphs.len() == glyph_infos.len());
    let mut measure_info = MeasureInfo {
        min_y: 0,
        max_y: 0,
        min_x: 0,
        max_x: 0,
        base_x: 0,
        global_min_x: 0,
        global_max_x: 0,
        error: None,
    };
    // Measure:
    for (glyph, glyph_info) in glyphs.iter().zip(glyph_infos.iter()) {
        let err = unsafe { ft_lib::FT_Load_Glyph(raw_face, glyph_info.codepoint, 0) };
        freetype::FtError::from_ft(err)?;
        if unsafe { *(*raw_face).glyph }.format != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE {
            panic!("Not an outline.");
        }
        let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*raw_face).glyph }.outline;
        let mut params = ft_lib::FT_Raster_Params_ {
            target: std::ptr::null(),
            source: std::ptr::null(),
            flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT).unwrap(),
            gray_spans: Some(measure_span),
            black_spans: None,
            bit_test: None,
            bit_set: None,
            user: unsafe { std::mem::transmute(&mut measure_info as *mut MeasureInfo) },
            clip_box: ft_lib::FT_BBox_ {
                xMin: 0,
                yMin: 0,
                xMax: 0,
                yMax: 0,
            },
        };
        {
            let ft_library = face.library().as_raw();
            let err = unsafe { ft_lib::FT_Outline_Render(ft_library, outline, &mut params) };
            freetype::FtError::from_ft(err)?;
        }
        let this_min_x = measure_info.base_x.checked_add(measure_info.min_x).unwrap();
        let this_max_x = measure_info.base_x.checked_add(measure_info.max_x).unwrap();
        measure_info.global_min_x = std::cmp::min(measure_info.global_min_x, this_min_x);
        measure_info.global_max_x = std::cmp::max(measure_info.global_max_x, this_max_x);
        measure_info.base_x += i32::from(glyph.x_advance >> 6);
    }
    let height = usize::try_from(
        measure_info.max_y.checked_sub(measure_info.min_y).and_then(|v| v.checked_add(1)).unwrap()
    ).unwrap();
    let width = usize::try_from(
        measure_info.global_max_x.checked_sub(measure_info.global_min_x).and_then(|v| v.checked_add(1)).unwrap()
    ).unwrap();
    let mut render_info = RenderInfo {
        base_y: measure_info.max_y,
        x: 0,
        image: Image::new(width, height),
        error: None,
    };
    // Render:
    for (glyph, glyph_info) in glyphs.iter().zip(glyph_infos.iter()) {
        let err = unsafe { ft_lib::FT_Load_Glyph(raw_face, glyph_info.codepoint, 0) };
        freetype::FtError::from_ft(err)?;
        if unsafe { *(*raw_face).glyph }.format != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE {
            panic!("Not an outline.");
        }
        let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*raw_face).glyph }.outline;
        let mut params = ft_lib::FT_Raster_Params_ {
            target: std::ptr::null(),
            source: std::ptr::null(),
            flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT).unwrap(),
            gray_spans: Some(render_span),
            black_spans: None,
            bit_test: None,
            bit_set: None,
            user: unsafe { std::mem::transmute(&mut render_info as *mut RenderInfo) },
            clip_box: ft_lib::FT_BBox_ {
                xMin: 0,
                yMin: 0,
                xMax: 0,
                yMax: 0,
            },
        };
        {
            let ft_library = face.library().as_raw();
            let err = unsafe { ft_lib::FT_Outline_Render(ft_library, outline, &mut params) };
            freetype::FtError::from_ft(err)?;
        }
        render_info.x += usize::try_from(glyph.x_advance >> 6).unwrap();
    }
    match render_info.error {
        Some(err) => Err(err),
        None => Ok(()),
    }.unwrap();
    unsafe {
        harfbuzz::hb_buffer_destroy(buffer);
    }
    Ok(render_info.image)
}

struct MeasureInfo {
    min_y: libc::c_int,
    max_y: libc::c_int,
    min_x: i32,
    max_x: i32,
    base_x: i32,
    global_min_x: i32,
    global_max_x: i32,
    error: Option<RenderError>,
}

extern "C" fn measure_span(y: libc::c_int, count: libc::c_int, spans: *const ft_lib::FT_Span, user: *mut libc::c_void) {
    let measure_info = unsafe {
        // This *cannot* leave this function.
        let measure_info: *mut MeasureInfo = std::mem::transmute(user);
        &mut *measure_info
    };
    measure_info.min_y = std::cmp::min(measure_info.min_y, y);
    measure_info.max_y = std::cmp::max(measure_info.max_y, y);
    let count = match usize::try_from(count) {
        Ok(v) => v,
        Err(err) => {
            measure_info.error = Some(RenderError::BadSpanCount(count, err));
            return;
        }
    };
    let spans = unsafe {
        let slice = std::ptr::slice_from_raw_parts(spans, count);
        &*slice
    };
    for span in spans {
        let span_x: i32 = span.x.into();
        measure_info.min_x = std::cmp::min(measure_info.min_x, span_x);
        let span_x_max = match span_x.checked_add(span.len.into()) {
            Some(v) => v,
            None => {
                measure_info.error = Some(RenderError::SpanXExceedsI32);
                return;
            }
        };
        measure_info.max_x = std::cmp::max(measure_info.max_x, span_x_max);
    }
}

struct RenderInfo {
    base_y: libc::c_int,
    x: usize,
    image: Image,
    error: Option<RenderError>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("FreeType passed the render function an invalid length for the span array: {0}; {1}")]
    BadSpanCount(libc::c_int, #[source] std::num::TryFromIntError),
    #[error("coordinate exceeds image bounds while rendering: {0}, {1}")]
    CoordinateOutsideImage(usize, usize),
    #[error("failed to compute Y from {0} - {1}")]
    SpanYComputeFailed(libc::c_int, libc::c_int),
    #[error("a span's X coord exceeded the bounds of an i32")]
    SpanXExceedsI32,
    #[error(transparent)]
    Freetype(#[from] freetype::FtError),
    #[error("Harfbuzz buffer allocation failed")]
    HarfbuzzBufferAllocFailed,
}

extern "C" fn render_span(y: libc::c_int, count: libc::c_int, spans: *const ft_lib::FT_Span, user: *mut libc::c_void) {
    let render_info = unsafe {
        // This *cannot* leave this function.
        let render_info: *mut RenderInfo = std::mem::transmute(user);
        &mut *render_info
    };
    let count = match usize::try_from(count) {
        Ok(v) => v,
        Err(err) => {
            render_info.error = Some(RenderError::BadSpanCount(count, err));
            return;
        }
    };
    let spans = unsafe {
        let slice = std::ptr::slice_from_raw_parts(spans, count);
        &*slice
    };
    let real_y = match render_info.base_y.checked_sub(y) {
        Some(y) => y,
        None => {
            render_info.error = Some(RenderError::SpanYComputeFailed(render_info.base_y, y));
            return;
        }
    };
    for span in spans {
        for x in i32::from(span.x) .. i32::from(span.x) + i32::from(span.len) {
            let x = render_info.x + usize::try_from(x).unwrap();
            let y = usize::try_from(real_y).unwrap();
            if !render_info.image.blend_pixel(x, y, span.coverage) {
                render_info.error = Some(RenderError::CoordinateOutsideImage(x, y));
                return;
            }
        }
    }
}

pub struct Image {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Image {
    fn new(width: usize, height: usize) -> Image {
        let mut data = Vec::new();
        data.resize(width * height, 0);
        Image {
            width,
            height,
            data,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    fn set_pixel(&mut self, x: usize, y: usize, value: u8) -> bool {
        if self.width <= x || self.height <= y {
            false
        } else {
            let index = self.width * y + x;
            self.data[index] = value;
            true
        }
    }

    fn blend_pixel(&mut self, x: usize, y: usize, value: u8) -> bool {
        if self.width <= x || self.height <= y {
            false
        } else {
            let index = self.width * y + x;
            let old_value = (self.data[index] as f32) / 255.;
            let value = (value as f32) / 255.;
            let blended = value + old_value * (1. - value);
            let blended = (blended * 255.) as u8;
            self.data[index] = blended;
            true
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.data
    }
}
