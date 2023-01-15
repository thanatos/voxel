use std::collections::HashMap;

use ::freetype::freetype as ft_lib;
use ft_lib::FT_F26Dot6;

use super::{freetype, GlyphMeasures};

pub struct GlyphCache {
    pub(super) for_height: FT_F26Dot6,
    cache: HashMap<std::os::raw::c_uint, CachedGlyph>,
}

pub(super) struct CachedGlyph {
    render: super::glyph_rendering::RenderedGlyph,
    measures: Option<GlyphMeasures>,
}

impl CachedGlyph {
    pub(super) fn spans(
        &self,
    ) -> impl Iterator<Item = (std::os::raw::c_int, ft_lib::FT_Span)> + '_ {
        self.render.spans()
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
        freetype::FtError::from_ft(err).map_err(CacheError::SetCharSize)?;

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
            let cached_glyph = {
                let mut ft_library_lock = face.library().lock().unwrap();
                let ft_library = ft_library_lock.as_mut_raw();
                let rendered_glyph = super::glyph_rendering::render_glyph(ft_library, raw_face, ch_as_glyph)
                    .map_err(|err| CacheError::RenderGlyph(ch, err))?;
                let measures = super::GlyphMeasuresBuilder::from_spans(rendered_glyph.spans());
                CachedGlyph {
                    render: rendered_glyph,
                    measures,
                }
            };
            cache.insert(ch_as_glyph, cached_glyph);
        }

        let mut total_size: usize = cache.values().map(|v| v.render.size_indirect()).sum();
        total_size += cache.capacity() * std::mem::size_of::<(std::os::raw::c_uint, CachedGlyph)>();
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

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("failed to set character size: {0}")]
    SetCharSize(freetype::FtError),
    #[error("failed to select charmap: {0}")]
    SelectCharmap(freetype::FtError),
    #[error("failed to load glyph for {0:?}: {1}")]
    LoadGlyph(char, freetype::FtError),
    #[error("failed to render glyph for {0:?}: {1}")]
    RenderGlyph(char, super::glyph_rendering::RenderGlyphError),
    #[error("overflow while counting spans/rows for {0:?}")]
    SpanCountOverflow(char),
}

const ALWAYS_CACHE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`~!@#$%^&*()-_=+[]{}\\|;:'\",.<>/?";
