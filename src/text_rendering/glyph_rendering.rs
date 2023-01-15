use std::any::Any;
use std::convert::TryFrom;
use std::os::raw::c_int;

use ::freetype::freetype as ft_lib;
use smallvec::SmallVec;

#[derive(Debug)]
pub struct RenderedGlyph {
    rows: Box<[(c_int, usize)]>,
    spans: Box<[ft_lib::FT_Span]>,
}

impl RenderedGlyph {
    pub fn spans(&self) -> impl Iterator<Item = (c_int, ft_lib::FT_Span)> + '_ {
        SpansIter::new(self)
    }

    /// A rough estimate, in B, of the size of indirect data. (I.e., that size not covered by
    /// `std::mem::size_of<RenderedGlyph>()`.)
    pub fn size_indirect(&self) -> usize {
        let s_r = self.rows.len() * std::mem::size_of::<(c_int, usize)>();
        let s_s = self.spans.len() * std::mem::size_of::<ft_lib::FT_Span>();
        s_r + s_s
    }
}

impl From<CapturedSpans> for RenderedGlyph {
    fn from(captured_spans: CapturedSpans) -> RenderedGlyph {
        RenderedGlyph {
            rows: captured_spans.rows.into_boxed_slice(),
            spans: captured_spans.spans.into_boxed_slice(),
        }
    }
}

enum SpansIterState {
    NotLastRow {
        y: c_int,
        next_row_y: c_int,
        next_row_idx: usize,
    },
    LastRow {
        y: std::os::raw::c_int,
    },
    Complete,
}

struct SpansIter<'a> {
    state: SpansIterState,
    rows_iter: std::slice::Iter<'a, (c_int, usize)>,
    span_iter: std::iter::Enumerate<std::slice::Iter<'a, ft_lib::FT_Span>>,
}

impl SpansIter<'_> {
    fn new(glyph: &RenderedGlyph) -> SpansIter {
        let mut rows_iter = glyph.rows.iter();
        let state = {
            let row = rows_iter.next();
            match row {
                Some((y, idx)) => {
                    assert!(*idx == 0);
                    match rows_iter.next() {
                        Some((ny, nidx)) => SpansIterState::NotLastRow {
                            y: *y,
                            next_row_y: *ny,
                            next_row_idx: *nidx,
                        },
                        None => SpansIterState::LastRow { y: *y },
                    }
                }
                None => SpansIterState::Complete,
            }
        };
        SpansIter {
            state,
            rows_iter,
            span_iter: glyph.spans.iter().enumerate(),
        }
    }
}

impl Iterator for SpansIter<'_> {
    type Item = (c_int, ft_lib::FT_Span);

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            SpansIterState::NotLastRow {
                y,
                next_row_y,
                next_row_idx,
            } => match self.span_iter.next() {
                Some((idx, s)) if idx == next_row_idx => {
                    match self.rows_iter.next() {
                        Some((ny, nidx)) => {
                            self.state = SpansIterState::NotLastRow {
                                y: next_row_y,
                                next_row_y: *ny,
                                next_row_idx: *nidx,
                            };
                        }
                        None => {
                            self.state = SpansIterState::LastRow { y: next_row_y };
                        }
                    }
                    Some((next_row_y, *s))
                }
                Some((_, s)) => Some((y, *s)),
                None => panic!("exhausted span iter before row iter?"),
            },
            SpansIterState::LastRow { y } => match self.span_iter.next() {
                Some((_, s)) => Some((y, *s)),
                None => {
                    self.state = SpansIterState::Complete;
                    None
                }
            },
            SpansIterState::Complete => None,
        }
    }
}

/// Where we write spans to, as FreeType feeds them to us. We allocate some decent sizes SmallVecs
/// to attempt to keep this on the stack. (Until we convert to a `RenderedGlyph`, which keeps these
/// on the heap. Ideally, we do a single allocation for each `SmallVec` for that conversion.)
///
/// In the case where we're only measuring glyphs, this can be used to possibly avoid any heap
/// allocations, beyond what FreeType requires.
#[derive(Debug)]
pub struct CapturedSpans {
    pub rows: SmallVec<[(c_int, usize); 16]>,
    pub spans: SmallVec<[ft_lib::FT_Span; 128]>,
}

impl CapturedSpans {
    pub fn new() -> CapturedSpans {
        CapturedSpans {
            rows: SmallVec::new(),
            spans: SmallVec::new(),
        }
    }
}

#[derive(Debug)]
struct CapturedSpansResult<'a> {
    capture: &'a mut CapturedSpans,
    panic: Option<Box<dyn Any + Send + 'static>>,
}

impl CapturedSpansResult<'_> {
    fn new(capture: &mut CapturedSpans) -> CapturedSpansResult {
        CapturedSpansResult {
            capture,
            panic: None,
        }
    }

    fn finish(self) {
        if let Some(panic) = self.panic {
            std::panic::resume_unwind(panic);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderGlyphError {
    #[error("failed to load glyph {0}: FreeType error: {1}")]
    LoadGlyphError(ft_lib::FT_UInt, ft_lib::FT_Error),
    #[error("glyph {0} was not an outline glyph")]
    GlyphWasNotAnOutline(ft_lib::FT_UInt),
    #[error("failed to render outline of glyph {0}: FreeType error: {1}")]
    OutlineRenderFailed(ft_lib::FT_UInt, ft_lib::FT_Error),
}

fn ft_err(ft_err: ft_lib::FT_Error) -> Result<(), ft_lib::FT_Error> {
    match ft_err {
        //ft_lib::FT_Err_Ok => Ok(()),
        0 => Ok(()),
        _ => Err(ft_err),
    }
}

pub fn render_glyph(
    library: ft_lib::FT_Library,
    face: ft_lib::FT_Face,
    glyph_index: ft_lib::FT_UInt,
) -> Result<RenderedGlyph, RenderGlyphError> {
    let mut captured_spans = CapturedSpans::new();
    render_glyph_raw(library, face, glyph_index, &mut captured_spans)?;
    Ok(RenderedGlyph::from(captured_spans))
}

pub fn render_glyph_raw(
    library: ft_lib::FT_Library,
    face: ft_lib::FT_Face,
    glyph_index: ft_lib::FT_UInt,
    captured_spans: &mut CapturedSpans,
) -> Result<(), RenderGlyphError> {
    let err = unsafe { ft_lib::FT_Load_Glyph(face, glyph_index, 0) };
    let err = ft_err(err).map_err(|err| RenderGlyphError::LoadGlyphError(glyph_index, err))?;

    if unsafe { *(*face).glyph }.format != ft_lib::FT_Glyph_Format_::FT_GLYPH_FORMAT_OUTLINE {
        return Err(RenderGlyphError::GlyphWasNotAnOutline(glyph_index));
    }
    let outline: *mut ft_lib::FT_Outline = &mut unsafe { *(*face).glyph }.outline;

    let mut captured_spans_result = CapturedSpansResult::new(captured_spans);
    let mut params = ft_lib::FT_Raster_Params_ {
        target: std::ptr::null(),
        source: std::ptr::null(),
        flags: i32::try_from(ft_lib::FT_RASTER_FLAG_AA | ft_lib::FT_RASTER_FLAG_DIRECT).expect(
            "assertion failed: we always expect FT_RASTER_FLAG_* to fit into an i32; \
                 this should never fail, but see: https://github.com/servo/rust-freetype/issues/65 \
                 prevents this from being actually infallible",
        ),
        gray_spans: Some(capture_spans),
        black_spans: None,
        bit_test: None,
        bit_set: None,
        user: unsafe {
            std::mem::transmute(&mut captured_spans_result as *mut CapturedSpansResult)
        },
        clip_box: ft_lib::FT_BBox_ {
            xMin: 0,
            yMin: 0,
            xMax: 0,
            yMax: 0,
        },
    };
    let err = unsafe { ft_lib::FT_Outline_Render(library, outline, &mut params) };
    // (the panic means that we really never meant to finish FT_Outline_Render.)
    /*
    log::debug!("Size of CSR? {}", std::mem::size_of::<CapturedSpansResult>());
    log::debug!(
        "Captured {} rows, {} spans; spilled? {:?}",
        captured_spans_result.capture.rows.len(),
        captured_spans_result.capture.spans.len(),
        captured_spans_result.capture.rows.spilled() || captured_spans_result.capture.spans.spilled(),
    );
    */
    captured_spans_result.finish();
    let err = ft_err(err).map_err(|err| RenderGlyphError::OutlineRenderFailed(glyph_index, err))?;

    Ok(())
}

extern "C" fn capture_spans(
    y: c_int,
    count: c_int,
    spans: *const ft_lib::FT_Span,
    user: *mut std::os::raw::c_void,
) {
    let captured_spans_result: &mut CapturedSpansResult = unsafe {
        // This *cannot* leave this function: it is borrowed in render_glyph, and cannot outlive
        // that borrow.
        let result: *mut CapturedSpansResult = std::mem::transmute(user);
        &mut *result
    };

    if captured_spans_result.panic.is_some() {
        return;
    }

    let mut inner_rows = std::panic::AssertUnwindSafe(&mut captured_spans_result.capture.rows);
    let mut inner_spans = std::panic::AssertUnwindSafe(&mut captured_spans_result.capture.spans);
    let result = std::panic::catch_unwind(move || {
        let spans_len = match usize::try_from(count) {
            Ok(v) => v,
            Err(_) => {
                panic!(
                    "FreeType passed us a `count` of spans ({:?}) that was not convertable to a \
                     usize.",
                    count,
                );
            }
        };
        let spans = unsafe { std::slice::from_raw_parts(spans, spans_len) };
        let last_index = inner_spans.len();
        inner_rows.push((y, last_index));
        inner_spans.extend_from_slice(spans);
    });
    match result {
        Ok(()) => (),
        Err(err) => captured_spans_result.panic = Some(err),
    }
}
