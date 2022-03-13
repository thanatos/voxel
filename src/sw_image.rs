use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// A 32-bit RGBA image held in RAM, manipulated on the CPU. (I.e., not an image on the GPU.)
#[derive(Clone)]
pub struct SwImage {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}

impl SwImage {
    pub fn new(width: u32, height: u32) -> SwImage {
        let pixel_count = usize::try_from(width)
            .expect("image width exceeded usize limits")
            .checked_mul(usize::try_from(height).expect("image height exceeded usize limits"))
            .expect("image bounds exceeded usize");
        let mut pixels = Vec::with_capacity(pixel_count);
        let default_pixel = Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        for p in std::iter::repeat(default_pixel).take(pixel_count) {
            pixels.push(p);
        }
        SwImage {
            width,
            height,
            pixels,
        }
    }

    pub fn pixels(&self) -> impl Iterator<Item = Pixel> + ExactSizeIterator + '_ {
        self.pixels.iter().copied()
    }

    fn index_for(&self, x: u32, y: u32) -> usize {
        if self.width <= x || self.height <= y {
            panic!("pixel at ({}, {}) lies outside image bounds", x, y);
        }
        let x = usize::try_from(x).expect("x exceeded usize limits");
        let y = usize::try_from(y).expect("y exceeded usize limits");
        let width = usize::try_from(self.width).expect("width exceeded usize limits");
        y.checked_mul(width).and_then(|v| v.checked_add(x)).expect("index overflowed usize")
    }

    pub fn blend_pixel(&mut self, x: u32, y: u32, value: Pixel) {
        let index = self.index_for(x, y);
        let old_value = self.pixels[index];
        self.pixels[index] = blend(value, old_value);
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Blend `a` over `b`.
#[inline]
fn blend(a: Pixel, b: Pixel) -> Pixel {
    let alpha_a = (a.a as f32) / 255.;
    let alpha_b_w = (b.a as f32) / 255. * (1. - alpha_a);
    let alpha_o = alpha_a + alpha_b_w;
    let o_r = ((a.r as f32) * alpha_a + (b.r as f32) * alpha_b_w) / alpha_o;
    let o_g = ((a.g as f32) * alpha_a + (b.g as f32) * alpha_b_w) / alpha_o;
    let o_b = ((a.b as f32) * alpha_a + (b.b as f32) * alpha_b_w) / alpha_o;
    Pixel {
        r: o_r as u8,
        g: o_g as u8,
        b: o_b as u8,
        a: (alpha_o * 255.) as u8,
    }
}
