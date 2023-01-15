use std::convert::TryFrom;

use ::freetype::freetype as ft_lib;

use crate::sw_image::{Pixel, SwImage};

pub mod cache;
pub mod glyph_rendering;
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
    face.set_char_size(14 << 6)?;
    let raw_face = face.as_mut_raw();
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
                log::debug!("Manually measuring glyph {}", glyph_info.codepoint);
                let mut captured_spans = glyph_rendering::CapturedSpans::new();
                {
                    let mut ft_library_lock = face.library().lock().unwrap();
                    let ft_library = ft_library_lock.as_mut_raw();
                    glyph_rendering::render_glyph_raw(
                        ft_library,
                        raw_face,
                        glyph_info.codepoint,
                        &mut captured_spans
                    )
                    .map_err(RenderError::RenderError)?;
                }
                let mut measure_builder = GlyphMeasuresBuilder::new();
                for (y, _) in captured_spans.rows.iter() {
                    measure_builder.measure_y(*y);
                }
                for span in captured_spans.spans.iter() {
                    measure_builder.measure_span(*span);
                }
                MaybeCachedGlyphMeasures::Computed(measure_builder.finish())
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
        color: Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        },
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
                log::debug!("Manually rendering glyph {}", glyph_info.codepoint);
                let rendered_glyph = {
                    let mut ft_library_lock = face.library().lock().unwrap();
                    let ft_library = ft_library_lock.as_mut_raw();
                    glyph_rendering::render_glyph(ft_library, raw_face, glyph_info.codepoint)
                        .map_err(RenderError::RenderError)?
                };
                for (y, span) in rendered_glyph.spans() {
                    render_span(&mut render_info, y, span)?;
                }
            }
        }
        render_info.x += u32::try_from(glyph.x_advance >> 6).unwrap();
    }
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

    fn from_spans(iter: impl Iterator<Item = (std::os::raw::c_int, ft_lib::FT_Span)>) -> Option<GlyphMeasures> {
        let mut builder = GlyphMeasuresBuilder::new();
        for (y, span) in iter {
            builder.measure_y(y);
            builder.measure_span(span);
        }
        builder.finish()
    }
}

struct RenderInfo {
    base_y: std::os::raw::c_int,
    x: u32,
    image: SwImage,
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
    #[error("Freetype failed to render glyph: {0}")]
    RenderError(glyph_rendering::RenderGlyphError),
}

fn render_cached_glyph(
    render_info: &mut RenderInfo,
    cached_glyph: &cache::CachedGlyph,
) -> Result<(), RenderError> {
    for (y, span) in cached_glyph.spans() {
        render_span(render_info, y, span)?;
    }
    Ok(())
}

fn render_span(
    render_info: &mut RenderInfo,
    y: std::os::raw::c_int,
    span: ft_lib::FT_Span,
) -> Result<(), RenderError> {
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
    Ok(())
}
