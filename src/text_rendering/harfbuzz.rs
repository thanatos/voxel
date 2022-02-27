#[link(name = "harfbuzz")]
extern {
    pub fn hb_ft_font_create(face: freetype::freetype::FT_Face) -> *mut hb_font_t;
    pub fn hb_ft_font_set_funcs(font: *mut hb_font_t);

    pub fn hb_buffer_create() -> *mut hb_buffer_t; 
    pub fn hb_buffer_destroy(buffer: *mut hb_buffer_t);
    pub fn hb_buffer_allocation_successful(buffer: *mut hb_buffer_t) -> hb_bool_t;
    pub fn hb_buffer_add_utf8(buffer: *mut hb_buffer_t, text: *const libc::c_char, text_length: libc::c_int, item_offset: libc::c_uint, item_length: libc::c_int);
    pub fn hb_buffer_set_direction(buffer: *mut hb_buffer_t, direction: hb_direction_t);
    pub fn hb_buffer_get_glyph_positions(buffer: *mut hb_buffer_t, length: *mut libc::c_uint) -> *mut hb_glyph_position_t;
    pub fn hb_buffer_get_glyph_infos(buffer: *mut hb_buffer_t, length: *mut libc::c_uint) -> *mut hb_glyph_info_t;

    pub fn hb_shape(font: *mut hb_font_t, buffer: *mut hb_buffer_t, features: *const hb_feature_t, num_features: libc::c_uint);
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct hb_bool_t(libc::c_int);

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
pub struct hb_font_t(u8);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hb_buffer_t(u8);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hb_tag_t(u32);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hb_feature_t {
    pub tag: hb_tag_t,
    pub value: u32,
    pub start: libc::c_uint,
    pub end: libc::c_uint,

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
