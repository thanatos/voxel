use std::any::Any;
use std::convert::TryFrom;

use ::freetype::freetype as ft_lib;

use crate::sw_image::{Pixel, SwImage};

pub mod cache;
pub mod freetype;
mod harfbuzz;

use cache::GlyphCache;

enum MaybeCachedGlyphMeasures<'a> {
    Cached(Option<&'a GlyphMeasures>),
    Computed(Option<GlyphMeasures>),
}

impl MaybeCachedGlyphMeasures<'_> {
    fn as_ref(&self) -> Option<&GlyphMeasures> {
        match self {
            MaybeCachedGlyphMeasures::Cached(m) => *m,
            MaybeCachedGlyphMeasures::Computed(m) => m.as_ref(),
        }
    }
}

pub struct FormattedText {
    text: String,
    // Maps starting index → color
    // The vector is always sorted by .0, as we always append successively higher indexes.
    // TODO: SmallVec?
    color_spans: Vec<(usize, Pixel)>,
}

impl FormattedText {
    pub fn new() -> FormattedText {
        FormattedText {
            text: String::new(),
            color_spans: Vec::new(),
        }
    }

    pub fn add_str(&mut self, s: &str, color: Pixel) {
        if self.color_spans.last().map(|(_, c)| *c) != Some(color) {
            self.color_spans.push((self.text.len(), color));
        }
        self.text.push_str(s);
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    // TODO: this is going to be O(n * log(n)) as we iterate through the glyphs in the string.
    // The glyphs should be mostly in order; a smarter lookup that remembers the last color span
    // could probably get O(n) in most cases.
    pub fn color_for_index(&self, index: usize) -> Pixel {
        match self.color_spans.binary_search_by_key(&index, |(i, _)| *i) {
            Ok(idx) => self.color_spans[idx].1,
            Err(idx) => self.color_spans[idx - 1].1,
        }
    }
}

pub fn render_text(
    text: &FormattedText,
    face: &mut freetype::FtFace,
    cache: &GlyphCache,
) -> Result<SwImage, RenderError> {
    // TODO: allow specifying the height
    assert!(cache.for_height == 14 << 6);
    let raw_face = face.as_mut_raw();
    let err = unsafe { ft_lib::FT_Set_Char_Size(raw_face, 0, 14 << 6, 0, 0) };
    freetype::FtError::from_ft(err)?;
    let mut hb_font = harfbuzz::HarfbuzzFont::from_freetype_face(raw_face);
    let mut buffer =
        harfbuzz::HarfbuzzBuffer::new().ok_or_else(|| RenderError::HarfbuzzBufferAllocFailed)?;
    buffer.set_direction(harfbuzz::hb_direction_t::HB_DIRECTION_LTR);
    buffer.add_str(text.as_str());
    harfbuzz::shape(&mut hb_font, &mut buffer);
    let (glyphs, glyph_infos) = buffer.glyph_positions_and_infos();
    assert!(glyphs.len() == glyph_infos.len());
    let mut measure_info = MeasureInfo::NoneYet;
    let mut base_x = 0;
    // Measure:
    for (glyph, glyph_info) in glyphs.iter().zip(glyph_infos.iter()) {
        let measures = match cache.get_glyph(glyph_info.codepoint) {
            Some(cached_glyph) => MaybeCachedGlyphMeasures::Cached(cached_glyph.measures()),
            None => {
                let err = unsafe { ft_lib::FT_Load_Glyph(raw_face, glyph_info.codepoint, 0) };
                freetype::FtError::from_ft(err)?;
                if unsafe { *(*raw_face).glyph }.format
                    != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE
                {
                    panic!("Not an outline.");
                }
                let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*raw_face).glyph }.outline;
                let mut measure_glyph_info = MeasureGlyphInfo::new();
                let mut params = ft_lib::FT_Raster_Params_ {
                    target: std::ptr::null(),
                    source: std::ptr::null(),
                    flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT)
                        .unwrap(),
                    gray_spans: Some(measure_glyph),
                    black_spans: None,
                    bit_test: None,
                    bit_set: None,
                    user: unsafe {
                        std::mem::transmute(&mut measure_glyph_info as *mut MeasureGlyphInfo)
                    },
                    clip_box: ft_lib::FT_BBox_ {
                        xMin: 0,
                        yMin: 0,
                        xMax: 0,
                        yMax: 0,
                    },
                };
                {
                    let mut ft_library_lock = face.library().lock().unwrap();
                    let ft_library = ft_library_lock.as_mut_raw();
                    let err =
                        unsafe { ft_lib::FT_Outline_Render(ft_library, outline, &mut params) };
                    freetype::FtError::from_ft(err)?;
                }
                MaybeCachedGlyphMeasures::Computed(measure_glyph_info.measures.finish())
            }
        };
        if let Some(measures) = measures.as_ref() {
            measure_info.merge(base_x, &measures);
        }
        base_x = base_x.checked_add(i32::from(glyph.x_advance >> 6)).unwrap();
    }
    let (base_y, width, height) = match measure_info {
        MeasureInfo::NoneYet => panic!("no measurements?"),
        MeasureInfo::Measures {
            min_y,
            max_y,
            global_min_x,
            global_max_x,
        } => {
            let height = u32::try_from(
                max_y
                    .checked_sub(min_y)
                    .and_then(|v| v.checked_add(1))
                    .unwrap(),
            )
            .unwrap();
            let width = u32::try_from(
                global_max_x
                    .checked_sub(global_min_x)
                    .and_then(|v| v.checked_add(1))
                    .unwrap(),
            )
            .unwrap();
            (max_y, width, height)
        }
    };
    let mut render_info = RenderInfo {
        base_y,
        x: 0,
        image: SwImage::new(width, height),
        error: None,
        panic: None,
        color: Pixel { r: 0, g: 0, b: 0, a: 0 },
    };
    // Render:
    for (glyph, glyph_info) in glyphs.iter().zip(glyph_infos.iter()) {
        let glyph_index_in_str = usize::try_from(glyph_info.cluster).unwrap();
        let color = text.color_for_index(glyph_index_in_str);
        render_info.color = color;
        match cache.get_glyph(glyph_info.codepoint) {
            Some(cached_glyph) => {
                render_cached_glyph(&mut render_info, cached_glyph)?;
            }
            None => {
                let err = unsafe { ft_lib::FT_Load_Glyph(raw_face, glyph_info.codepoint, 0) };
                freetype::FtError::from_ft(err)?;
                if unsafe { *(*raw_face).glyph }.format
                    != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE
                {
                    panic!("Not an outline.");
                }
                let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*raw_face).glyph }.outline;
                let mut params = ft_lib::FT_Raster_Params_ {
                    target: std::ptr::null(),
                    source: std::ptr::null(),
                    flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT)
                        .unwrap(),
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
                    let mut ft_library_lock = face.library().lock().unwrap();
                    let ft_library = ft_library_lock.as_mut_raw();
                    let err =
                        unsafe { ft_lib::FT_Outline_Render(ft_library, outline, &mut params) };
                    freetype::FtError::from_ft(err)?;
                }
                if let Some(p) = render_info.panic {
                    std::panic::resume_unwind(p);
                }
            }
        }
        render_info.x += u32::try_from(glyph.x_advance >> 6).unwrap();
    }
    match render_info.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
    .unwrap();
    Ok(render_info.image)
}

#[derive(Debug)]
enum MeasureInfo {
    NoneYet,
    Measures {
        min_y: std::os::raw::c_int,
        max_y: std::os::raw::c_int,
        global_min_x: i32,
        global_max_x: i32,
    },
}

impl MeasureInfo {
    fn merge(&mut self, base_x: i32, measures: &GlyphMeasures) {
        match self {
            MeasureInfo::NoneYet => {
                *self = MeasureInfo::Measures {
                    min_y: measures.min_y,
                    max_y: measures.max_y,
                    global_min_x: base_x.checked_add(i32::from(measures.min_x)).unwrap(),
                    global_max_x: base_x.checked_add(i32::from(measures.max_x)).unwrap(),
                }
            }
            MeasureInfo::Measures {
                min_y,
                max_y,
                global_min_x,
                global_max_x,
            } => {
                *min_y = std::cmp::min(*min_y, measures.min_y);
                *max_y = std::cmp::max(*max_y, measures.max_y);
                let this_min_x = base_x.checked_add(i32::from(measures.min_x)).unwrap();
                let this_max_x = base_x.checked_add(i32::from(measures.max_x)).unwrap();
                *global_min_x = std::cmp::min(*global_min_x, this_min_x);
                *global_max_x = std::cmp::max(*global_max_x, this_max_x);
            }
        }
    }
}

struct GlyphMeasures {
    min_y: std::os::raw::c_int,
    max_y: std::os::raw::c_int,
    min_x: std::os::raw::c_short,
    max_x: std::os::raw::c_short,
}

#[derive(Debug)]
struct GlyphMeasuresBuilder {
    y: Option<(std::os::raw::c_int, std::os::raw::c_int)>,
    x: Option<(std::os::raw::c_short, std::os::raw::c_short)>,
}

impl GlyphMeasuresBuilder {
    fn new() -> GlyphMeasuresBuilder {
        GlyphMeasuresBuilder { y: None, x: None }
    }

    fn measure_y(&mut self, y: std::os::raw::c_int) {
        match &mut self.y {
            Some((min_y, max_y)) => {
                *min_y = std::cmp::min(*min_y, y);
                *max_y = std::cmp::max(*max_y, y);
            }
            None => {
                self.y = Some((y, y));
            }
        }
    }
    fn measure_span(&mut self, span: ft_lib::FT_Span_) {
        let span_x = span.x;
        let span_x_end = span
            .x
            .checked_add(std::os::raw::c_short::try_from(span.len).unwrap())
            .unwrap();
        match &mut self.x {
            Some((min_x, max_x)) => {
                *min_x = std::cmp::min(*min_x, span_x);
                *max_x = std::cmp::max(*max_x, span_x_end);
            }
            None => {
                self.x = Some((span_x, span_x_end));
            }
        }
    }

    fn finish(&self) -> Option<GlyphMeasures> {
        match (self.y, self.x) {
            (Some((min_y, max_y)), Some((min_x, max_x))) => Some(GlyphMeasures {
                min_y,
                max_y,
                min_x,
                max_x,
            }),
            (None, None) => None,
            _ => panic!(),
        }
    }
}

struct MeasureGlyphInfo {
    measures: GlyphMeasuresBuilder,
    error: Option<RenderError>,
}

impl MeasureGlyphInfo {
    fn new() -> MeasureGlyphInfo {
        MeasureGlyphInfo {
            measures: GlyphMeasuresBuilder::new(),
            error: None,
        }
    }
}

extern "C" fn measure_glyph(
    y: std::os::raw::c_int,
    count: std::os::raw::c_int,
    spans: *const ft_lib::FT_Span,
    user: *mut std::os::raw::c_void,
) {
    let measure_info = unsafe {
        // This *cannot* leave this function.
        let measure_info: *mut MeasureGlyphInfo = std::mem::transmute(user);
        &mut *measure_info
    };
    measure_info.measures.measure_y(y);
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
        measure_info.measures.measure_span(*span);
    }
}

struct RenderInfo {
    base_y: std::os::raw::c_int,
    x: u32,
    image: SwImage,
    error: Option<RenderError>,
    panic: Option<Box<dyn Any + Send + 'static>>,
    color: Pixel,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("FreeType passed the render function an invalid length for the span array: {0}; {1}")]
    BadSpanCount(std::os::raw::c_int, #[source] std::num::TryFromIntError),
    #[error("coordinate exceeds image bounds while rendering: {0}, {1}")]
    CoordinateOutsideImage(usize, usize),
    #[error("failed to compute Y from {0} - {1}")]
    SpanYComputeFailed(std::os::raw::c_int, std::os::raw::c_int),
    #[error("a span's X coord exceeded the bounds of an i32")]
    SpanXExceedsI32,
    #[error(transparent)]
    Freetype(#[from] freetype::FtError),
    #[error("Harfbuzz buffer allocation failed")]
    HarfbuzzBufferAllocFailed,
}

fn render_cached_glyph(
    render_info: &mut RenderInfo,
    cached_glyph: &cache::CachedGlyph,
) -> Result<(), RenderError> {
    for (y, span) in cached_glyph.spans() {
        let real_y = match render_info.base_y.checked_sub(y) {
            Some(y) => y,
            None => {
                return Err(RenderError::SpanYComputeFailed(render_info.base_y, y));
            }
        };
        let y = u32::try_from(real_y).unwrap();
        let color = {
            let mut color = render_info.color;
            // FIXME: we ignore the specified alpha
            color.a = span.coverage;
            color
        };
        for x in i32::from(span.x)..i32::from(span.x).checked_add(i32::from(span.len)).unwrap() {
            let x = render_info.x + u32::try_from(x).unwrap();
            render_info.image.blend_pixel(x, y, color);
        }
    }
    Ok(())
}

extern "C" fn render_span(
    y: std::os::raw::c_int,
    count: std::os::raw::c_int,
    spans: *const ft_lib::FT_Span,
    user: *mut std::os::raw::c_void,
) {
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
    let inner = (&mut render_info.image, render_info.color, render_info.x);
    let mut inner_render_info = std::panic::AssertUnwindSafe(inner);
    let result = std::panic::catch_unwind(move || {
        let (image, color, render_x) = inner_render_info.0;
        for span in spans {
            for x in i32::from(span.x)..i32::from(span.x) + i32::from(span.len) {
                let x = render_x + u32::try_from(x).unwrap();
                let y = u32::try_from(real_y).unwrap();
                let color = {
                    let mut color = color;
                    // FIXME: we ignore the specified alpha
                    color.a = span.coverage;
                    color
                };
                image.blend_pixel(x, y, color);
            }
        }
    });

    match result {
        Ok(()) => (),
        Err(err) => render_info.panic = Some(err),
    }
}
