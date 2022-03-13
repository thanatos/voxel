//! Safe wrappers around the required bits of Harfbuzz.
//!
//! Harfbuzz is a text shaping library. It converts a string of unicode characters/code points into
//! a list of font glyphs & positions.
//!
//! Harfbuzz's API is inherently unsafe, being written in C.

use std::convert::TryFrom;

/// A font, as understood by Harfbuzz.
pub struct HarfbuzzFont {
    inner: *mut hb_font_t,
}

impl HarfbuzzFont {
    pub fn from_freetype_face(face: freetype::freetype::FT_Face) -> HarfbuzzFont {
        let font = unsafe { hb_ft_font_create_referenced(face) };
        if font == unsafe { hb_font_get_empty() } {
            panic!("failed to allocate Harfbuzz font");
        }
        // XXX: This call can fail, but it doesn't report that to us!
        unsafe { hb_ft_font_set_funcs(font) };

        HarfbuzzFont {
            inner: font,
        }
    }
}

impl Drop for HarfbuzzFont {
    fn drop(&mut self) {
        unsafe { hb_font_destroy(self.inner) };
        self.inner = std::ptr::null_mut();
    }
}

/// A "buffer" in Harfbuzz contains all the data required to shape a set of text, including the
/// text itself.
pub struct HarfbuzzBuffer {
    inner: *mut hb_buffer_t,
}

impl HarfbuzzBuffer {
    pub fn new() -> Option<HarfbuzzBuffer> {
        let buffer = unsafe { hb_buffer_create() };
        let success = unsafe { hb_buffer_allocation_successful(buffer) };
        if success.as_bool() {
            Some(HarfbuzzBuffer {
                inner: buffer,
            })
        } else {
            None
        }
    }

    pub fn set_direction(&mut self, direction: hb_direction_t) {
        unsafe { hb_buffer_set_direction(self.inner, direction) };
    }

    pub fn add_str(&mut self, s: &str) {
        // XXX: While there is hb_buffer_add_utf8, it has all sort of defects.
        //
        // * it uses the wrong types for sizes (`int`, instead of `s?size_t`)
        // * it uses the wrong types for offsets (`unsigned int`, instead of `size_t`)
        // * it uses the wrong type for the UTF-8 text buffer (`const char *`)
        //   (…internally, it immediately casts it to the proper type, too…)
        // * while it attempts to preallocate, it fails to compute the amount correctly.
        //
        // In particular, we can easily determine the exact amount to pre-allocate, and we can do
        // the UTF-8 decode in Rust.

        let current_len = {
            let len = unsafe { hb_buffer_get_length(self.inner) };
            // XXX: Wrong type from Harfbuzz; it returns an `unsigned int` :(
            usize::try_from(len).unwrap()
        };

        // hb_buffer_add doesn't touch the buffer content type, so we need to handle that
        // ourselves.
        // First, do the assert_unicode() check:
        let current_type = unsafe { hb_buffer_get_content_type(self.inner) };
        match (current_type, current_len) {
            (hb_buffer_content_type_t::HB_BUFFER_CONTENT_TYPE_INVALID, 0) => (),
            (hb_buffer_content_type_t::HB_BUFFER_CONTENT_TYPE_UNICODE, _) => (),
            _ => {
                panic!("Harfbuzz buffer was not a Unicode buffer in add_str() call");
            }
        }

        let code_points = s.chars().count();
        let new_len = code_points.checked_add(current_len).unwrap();
        // XXX Again, Harfbuzz uses the wrong type.
        let new_len = std::os::raw::c_uint::try_from(new_len).unwrap();
        let success = unsafe { hb_buffer_pre_allocate(self.inner, new_len) };
        if !success.as_bool() {
            panic!("Harfbuzz buffer failed to pre-allocate {} bytes", new_len);
        }
        for (idx, ch) in s.char_indices() {
            let ch = ch.into();
            // XXX: And once more, wrong type.
            let idx = std::os::raw::c_uint::try_from(idx).unwrap();
            unsafe { hb_buffer_add(self.inner, ch, idx) };
        }

        // Last, set the content type:
        unsafe {
            hb_buffer_set_content_type(
                self.inner,
                hb_buffer_content_type_t::HB_BUFFER_CONTENT_TYPE_UNICODE,
            );
        }
    }

    // XXX: This is combined into one function as it can't be two, due to taking &mut; Harfbuzz's
    // API doesn't gaurantee it won't touch the data, and in fact, `…get_glyph_positions` *does*
    // mutate the buffer!
    //
    // Since you often want both (but can't call two `&mut self` functions at the same time)
    // they're combined.
    pub fn glyph_positions_and_infos(&mut self) -> (&[hb_glyph_position_t], &[hb_glyph_info_t]) {
        let positions = {
            let mut len = 0;
            let glyphs = unsafe { hb_buffer_get_glyph_positions(self.inner, &mut len) };
            // XXX: Wrong type from Harfbuzz
            let len = usize::try_from(len).unwrap();
            unsafe { std::slice::from_raw_parts(glyphs, len) }
        };

        let infos = {
            let mut len = 0;
            let glyphs = unsafe { hb_buffer_get_glyph_infos(self.inner, &mut len) };
            // XXX: Wrong type from Harfbuzz
            let len = usize::try_from(len).unwrap();
            unsafe { std::slice::from_raw_parts(glyphs, len) }
        };

        (positions, infos)
    }
}

impl Drop for HarfbuzzBuffer {
    fn drop(&mut self) {
        unsafe { hb_buffer_destroy(self.inner) };
        self.inner = std::ptr::null_mut();
    }
}

/// Shape the given `buffer` with the given `font`.
pub fn shape(font: &mut HarfbuzzFont, buffer: &mut HarfbuzzBuffer) {
    let hb_font_raw = font.inner;
    let hb_buffer_raw = buffer.inner;
    unsafe {
        hb_shape(hb_font_raw, hb_buffer_raw, std::ptr::null(), 0);
    }
}

#[link(name = "harfbuzz")]
extern {
    fn hb_font_get_empty() -> *mut hb_font_t;
    fn hb_font_destroy(font: *mut hb_font_t);
    fn hb_ft_font_create_referenced(face: freetype::freetype::FT_Face) -> *mut hb_font_t;
    fn hb_ft_font_set_funcs(font: *mut hb_font_t);

    fn hb_buffer_create() -> *mut hb_buffer_t;
    fn hb_buffer_destroy(buffer: *mut hb_buffer_t);
    fn hb_buffer_allocation_successful(buffer: *mut hb_buffer_t) -> hb_bool_t;
    fn hb_buffer_pre_allocate(buffer: *mut hb_buffer_t, size: std::os::raw::c_uint) -> hb_bool_t;
    fn hb_buffer_add(buffer: *mut hb_buffer_t, codepoint: hb_codepoint_t, cluster: std::os::raw::c_uint);
    fn hb_buffer_get_length(buffer: *mut hb_buffer_t) -> std::os::raw::c_uint;
    fn hb_buffer_get_content_type(buffer: *mut hb_buffer_t) -> hb_buffer_content_type_t;
    fn hb_buffer_set_content_type(buffer: *mut hb_buffer_t, content_type: hb_buffer_content_type_t);
    fn hb_buffer_set_direction(buffer: *mut hb_buffer_t, direction: hb_direction_t);
    fn hb_buffer_get_glyph_positions(buffer: *mut hb_buffer_t, length: *mut std::os::raw::c_uint) -> *mut hb_glyph_position_t;
    fn hb_buffer_get_glyph_infos(buffer: *mut hb_buffer_t, length: *mut std::os::raw::c_uint) -> *mut hb_glyph_info_t;

    fn hb_shape(font: *mut hb_font_t, buffer: *mut hb_buffer_t, features: *const hb_feature_t, num_features: std::os::raw::c_uint);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct hb_bool_t(std::os::raw::c_int);

impl hb_bool_t {
    pub fn as_bool(self) -> bool {
        match self.0 {
            0 => false,
            1 => true,
            _ => panic!("hb_boot_t was not 0 or 1"),
        }
    }
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct hb_font_t(u8);

#[repr(C)]
#[allow(non_camel_case_types)]
struct hb_buffer_t(u8);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hb_tag_t(u32);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hb_feature_t {
    pub tag: hb_tag_t,
    pub value: u32,
    pub start: std::os::raw::c_uint,
    pub end: std::os::raw::c_uint,

}

#[repr(C)]
#[allow(non_camel_case_types)]
#[allow(unused)]
enum hb_buffer_content_type_t {
    HB_BUFFER_CONTENT_TYPE_INVALID = 0,
    HB_BUFFER_CONTENT_TYPE_UNICODE,
    HB_BUFFER_CONTENT_TYPE_GLYPHS,
}

#[repr(C)]
#[allow(non_camel_case_types)]
#[allow(unused)]
pub enum hb_direction_t {
    HB_DIRECTION_INVALID = 0,
    HB_DIRECTION_LTR = 4,
    HB_DIRECTION_RTL,
    HB_DIRECTION_TTB,
    HB_DIRECTION_BTT,
}

#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct hb_glyph_position_t {
    pub x_advance: hb_position_t,
    pub y_advance: hb_position_t,
    pub x_offset: hb_position_t,
    pub y_offset: hb_position_t,
    // private, undocumented
    var: hb_var_int_t,
}
#[allow(non_camel_case_types)]
pub type hb_position_t = i32;

#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct hb_glyph_info_t {
    pub codepoint: hb_codepoint_t,
    // private, undocumented
    mask: hb_mask_t,
    pub cluster: u32,
    // private, undocumented
    var1: hb_var_int_t,
    // private, undocumented
    var2: hb_var_int_t,
}
#[allow(non_camel_case_types)]
pub type hb_codepoint_t = u32;

// This isn't a full representation of this type, but it should be compatible.
#[allow(non_camel_case_types)]
type hb_var_int_t = u32;
#[allow(non_camel_case_types)]
type hb_mask_t = u32;
