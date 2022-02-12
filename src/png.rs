use std::io::Write;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub trait ToPixel {
    fn to_pixel(&self) -> Pixel;
}

impl ToPixel for Pixel {
    fn to_pixel(&self) -> Pixel {
        *self
    }
}

pub fn write_png<W: Write, I: IntoIterator<Item = P>, P: ToPixel>(
    write: W,
    width: u32,
    height: u32,
    pixels: I,
) -> Result<(), png::EncodingError> {
    let pixel_data = {
        let mut pixel_data = Vec::new();
        for pixel in pixels {
            let pixel = pixel.to_pixel();
            pixel_data.push(pixel.r);
            pixel_data.push(pixel.g);
            pixel_data.push(pixel.b);
            pixel_data.push(pixel.a);
        }
        pixel_data
    };

    let mut encoder = png::Encoder::new(write, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixel_data)?;
    writer.finish()
}
