use std::any::Any;
use std::collections::HashMap;
use std::convert::TryFrom;

use ::freetype::freetype as ft_lib;
use ft_lib::FT_F26Dot6;

use super::{freetype, GlyphMeasures};

pub struct GlyphCache {
    pub(super) for_height: FT_F26Dot6,
    cache: HashMap<std::os::raw::c_uint, CachedGlyph>,
}

pub(super) struct CachedGlyph {
    capture: CapturedSpans,
    measures: Option<GlyphMeasures>,
}

impl CachedGlyph {
    pub(super) fn spans(
        &self,
    ) -> impl Iterator<Item = (std::os::raw::c_int, ft_lib::FT_Span)> + '_ {
        self.capture.spans()
    }

    pub(super) fn measures(&self) -> Option<&GlyphMeasures> {
        self.measures.as_ref()
    }
}

impl GlyphCache {
    pub fn empty(for_height: FT_F26Dot6) -> GlyphCache {
        GlyphCache {
            for_height,
            cache: HashMap::new(),
        }
    }

    pub fn new(face: &mut freetype::FtFace, height: FT_F26Dot6) -> Result<GlyphCache, CacheError> {
        let mut cache = HashMap::new();

        let raw_face = face.as_mut_raw();

        let err = unsafe { ft_lib::FT_Set_Char_Size(raw_face, 0, height, 0, 0) };

        let err = unsafe {
            ft_lib::FT_Select_Charmap(raw_face, ft_lib::FT_Encoding_::FT_ENCODING_UNICODE)
        };
        freetype::FtError::from_ft(err).map_err(CacheError::SelectCharmap)?;

        for ch in ALWAYS_CACHE.chars() {
            let ch_as_ul = ft_lib::FT_ULong::from(ch);
            let ch_as_glyph = unsafe { ft_lib::FT_Get_Char_Index(raw_face, ch_as_ul) };
            if ch_as_glyph == 0 {
                // This character lacks a glyph in the given font, & thus cannot be cached.
                continue;
            }
            let err = unsafe { ft_lib::FT_Load_Glyph(raw_face, ch_as_glyph, 0) };
            freetype::FtError::from_ft(err).map_err(|err| CacheError::LoadGlyph(ch, err))?;

            if unsafe { *(*raw_face).glyph }.format
                != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE
            {
                panic!("Not an outline.");
            }
            let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*raw_face).glyph }.outline;

            let mut captured_spans = CapturedSpansResult::new();
            let mut params = ft_lib::FT_Raster_Params_ {
                target: std::ptr::null(),
                source: std::ptr::null(),
                flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT)
                    .expect(
                        "assertion failed: we always expect FT_RASTER_FLAG_* to fit into an i32",
                    ),
                gray_spans: Some(capture_spans),
                black_spans: None,
                bit_test: None,
                bit_set: None,
                user: unsafe {
                    std::mem::transmute(&mut captured_spans as *mut CapturedSpansResult)
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
                let err = unsafe { ft_lib::FT_Outline_Render(ft_library, outline, &mut params) };
                freetype::FtError::from_ft(err).map_err(|err| CacheError::RenderGlyph(ch, err))?;
            }
            let (captured_spans, measures) = captured_spans.finish();
            let mut captured_spans = captured_spans;
            captured_spans.rows.shrink_to_fit();
            captured_spans.spans.shrink_to_fit();
            println!("CapturedSpans for {}: {:?}", ch, captured_spans);
            println!("CapturedSpans for {}: {:?}", ch, captured_spans.size());
            let cached_glyph = CachedGlyph {
                capture: captured_spans,
                measures,
            };
            cache.insert(ch_as_glyph, cached_glyph);
        }

        let mut total_size: usize = cache.values().map(|v| v.capture.size()).sum();
        total_size += cache.capacity() * std::mem::size_of::<(char, CachedGlyph)>();
        log::debug!("Cached {} font glyphs, {}B.", cache.len(), total_size);

        Ok(GlyphCache {
            for_height: height,
            cache,
        })
    }

    pub(super) fn get_glyph(&self, glyph: std::os::raw::c_uint) -> Option<&CachedGlyph> {
        self.cache.get(&glyph)
    }
}

#[derive(Debug)]
pub struct CapturedSpans {
    // (y, idx of row's spans)
    rows: Vec<(std::os::raw::c_int, usize)>,
    spans: Vec<ft_lib::FT_Span>,
}

impl CapturedSpans {
    fn size(&self) -> usize {
        let s_r = self.rows.capacity() * std::mem::size_of::<(std::os::raw::c_int, usize)>();
        let s_s = self.spans.capacity() * std::mem::size_of::<ft_lib::FT_Span>();
        s_r + s_s
    }

    fn spans(&self) -> impl Iterator<Item = (std::os::raw::c_int, ft_lib::FT_Span)> + '_ {
        CapturedSpansSpanIter::new(self)
    }
}

struct CapturedSpansSpanIter<'a> {
    state: CapturedSpansSpanIterState,
    rows_iter: std::slice::Iter<'a, (std::os::raw::c_int, usize)>,
    span_iter: std::iter::Enumerate<std::slice::Iter<'a, ft_lib::FT_Span>>,
}

enum CapturedSpansSpanIterState {
    NotLastRow {
        y: std::os::raw::c_int,
        next_row: std::os::raw::c_int,
        next_row_idx: usize,
    },
    LastRow {
        y: std::os::raw::c_int,
    },
    Complete,
}

impl CapturedSpansSpanIter<'_> {
    fn new(cp: &CapturedSpans) -> CapturedSpansSpanIter {
        let mut rows_iter = cp.rows.iter();
        let state = {
            let row = rows_iter.next();
            match row {
                Some((y, idx)) => {
                    assert!(*idx == 0);
                    match rows_iter.next() {
                        Some((ny, nidx)) => CapturedSpansSpanIterState::NotLastRow {
                            y: *y,
                            next_row: *ny,
                            next_row_idx: *nidx,
                        },
                        None => CapturedSpansSpanIterState::LastRow { y: *y },
                    }
                }
                None => CapturedSpansSpanIterState::Complete,
            }
        };
        CapturedSpansSpanIter {
            state,
            rows_iter,
            span_iter: cp.spans.iter().enumerate(),
        }
    }
}

impl Iterator for CapturedSpansSpanIter<'_> {
    type Item = (std::os::raw::c_int, ft_lib::FT_Span);

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            CapturedSpansSpanIterState::NotLastRow {
                y,
                next_row,
                next_row_idx,
            } => match self.span_iter.next() {
                Some((idx, s)) if idx == next_row_idx => {
                    match self.rows_iter.next() {
                        Some((ny, nidx)) => {
                            self.state = CapturedSpansSpanIterState::NotLastRow {
                                y: next_row,
                                next_row: *ny,
                                next_row_idx: *nidx,
                            };
                        }
                        None => {
                            self.state = CapturedSpansSpanIterState::LastRow { y: next_row };
                        }
                    }
                    Some((y, *s))
                }
                Some((_, s)) => Some((y, *s)),
                None => panic!("exhausted span iter before row iter?"),
            },
            CapturedSpansSpanIterState::LastRow { y } => match self.span_iter.next() {
                Some((_, s)) => Some((y, *s)),
                None => {
                    self.state = CapturedSpansSpanIterState::Complete;
                    None
                }
            },
            CapturedSpansSpanIterState::Complete => None,
        }
    }
}

#[derive(Debug)]
struct CapturedSpansResult {
    capture: CapturedSpans,
    measures: super::GlyphMeasuresBuilder,
    panic: Option<Box<dyn Any + Send + 'static>>,
}

impl CapturedSpansResult {
    fn new() -> CapturedSpansResult {
        CapturedSpansResult {
            capture: CapturedSpans {
                rows: Vec::new(),
                spans: Vec::new(),
            },
            measures: super::GlyphMeasuresBuilder::new(),
            panic: None,
        }
    }

    fn finish(self) -> (CapturedSpans, Option<GlyphMeasures>) {
        if let Some(p) = self.panic {
            std::panic::resume_unwind(p);
        }
        (self.capture, self.measures.finish())
    }
}

extern "C" fn capture_spans(
    y: std::os::raw::c_int,
    count: std::os::raw::c_int,
    spans: *const ft_lib::FT_Span,
    user: *mut std::os::raw::c_void,
) {
    let captured_spans = unsafe {
        // This *cannot* leave this function.
        let counts: *mut CapturedSpansResult = std::mem::transmute(user);
        &mut *counts
    };
    let mut inner_captured_spans = std::panic::AssertUnwindSafe(&mut captured_spans.capture);
    let mut inner_glyph_measures = std::panic::AssertUnwindSafe(&mut captured_spans.measures);
    let result = std::panic::catch_unwind(move || {
        inner_glyph_measures.measure_y(y);
        let spans = unsafe {
            let count = match usize::try_from(count) {
                Ok(v) => v,
                Err(_) => {
                    panic!("FreeType passed us a `count` of spans that was not convertable to a usize.")
                }
            };
            let slice = std::ptr::slice_from_raw_parts(spans, count);
            &*slice
        };

        for span in spans {
            inner_glyph_measures.measure_span(*span);
        }
        let last_index = inner_captured_spans.spans.len();
        inner_captured_spans.rows.push((y, last_index));
        inner_captured_spans.spans.extend_from_slice(spans);
    });
    match result {
        Ok(()) => (),
        Err(err) => captured_spans.panic = Some(err),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("failed to select charmap: {0}")]
    SelectCharmap(freetype::FtError),
    #[error("failed to load glyph for {0:?}: {1}")]
    LoadGlyph(char, freetype::FtError),
    #[error("failed to render glyph for {0:?}: {1}")]
    RenderGlyph(char, freetype::FtError),
    #[error("overflow while counting spans/rows for {0:?}")]
    SpanCountOverflow(char),
}

const ALWAYS_CACHE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`~!@#$%^&*()-_=+[]{}\\|;:'\",.<>/?";
