//! Code for loading the MagicaVoxel .VOX file format.
//!
//! There's some [very sparse documentation of the format](https://github.com/ephtracy/voxel-model)
//! but you'll see a lot of notes below where the documentation has holes.

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::io::{self, Read, Seek};

/// Read a MagicaVoxel .VOX file from the given `Read`
pub fn from_reader<R: Read + Seek>(mut reader: R) -> io::Result<Chunk> {
    parse_header(&mut reader)?;

    let mut chunk_stack = Vec::<ParseState>::new();

    let chunk_header = read_chunk_header(&mut reader)?;
    let chunk = parse_chunk(
        &mut reader,
        chunk_header.chunk_id,
        chunk_header.chunk_content_len,
    )?;
    chunk_stack.push(ParseState {
        child_chunk_size_remaining: chunk_header.chunk_children_len,
        chunk,
    });

    let mut steps = 0;
    let main_chunk = loop {
        let should_pop = {
            let top = chunk_stack
                .last()
                .expect("stack should always have states on it");
            top.child_chunk_size_remaining == 0
        };
        if should_pop {
            println!("Pop.");
            let top = chunk_stack
                .pop()
                .expect("stack should always have states on it");
            match chunk_stack.last_mut() {
                Some(ps) => {
                    ps.chunk.children.push(top.chunk);
                    // We need to start at the loop top again, in case we're finishing multiple
                    // chunks at the same time.
                    continue;
                }
                None => break top.chunk,
            }
        }

        println!(
            "At: {}, stack depth: {}",
            reader.stream_position()?,
            chunk_stack.len()
        );
        let top = chunk_stack
            .last_mut()
            .expect("stack should always have states on it");
        println!("Size remaining: {}", top.child_chunk_size_remaining);
        if top.child_chunk_size_remaining < 12 {
            return Err(invalid_data(
                "too few bytes remaining in parent chunk to continue to read in children",
            ));
        }

        let chunk_header = read_chunk_header(&mut reader)?;
        top.child_chunk_size_remaining -= 12;
        top.child_chunk_size_remaining = top
            .child_chunk_size_remaining
            .checked_sub(chunk_header.chunk_content_len)
            .ok_or_else(|| {
                invalid_data(
                "chunk content length exceeded the length of all sub-chunks in the parent chunk",
            )
            })?;
        let chunk = parse_chunk(
            &mut reader,
            chunk_header.chunk_id,
            chunk_header.chunk_content_len,
        )?;
        top.child_chunk_size_remaining = top
            .child_chunk_size_remaining
            .checked_sub(chunk_header.chunk_children_len)
            .ok_or_else(|| {
                invalid_data(
                "chunk children length exceeded the length of all sub-chunks in the parent chunk",
            )
            })?;
        println!(
            "Pushing: {:?} (content len = {}, children len = {})",
            chunk_header.chunk_id, chunk_header.chunk_content_len, chunk_header.chunk_children_len,
        );
        chunk_stack.push(ParseState {
            child_chunk_size_remaining: chunk_header.chunk_children_len,
            chunk,
        });
        steps += 1;
        if steps > 400 {
            panic!("...");
        }
    };

    Ok(main_chunk)
}

struct ParseState {
    child_chunk_size_remaining: u32,
    chunk: Chunk,
}

/// Read the b"VOX [version]" header.
fn parse_header<R: Read>(mut reader: R) -> io::Result<()> {
    let mut buf = [0u8; 8];
    let bytes_read = reader.read_exact(&mut buf)?;
    if &buf[..4] != b"VOX " {
        Err(invalid_data(".vox magic not found"))
    } else if 150
        != u32::from_le_bytes(
            buf[4..]
                .try_into()
                .expect("slice should have been length 4"),
        )
    {
        Err(invalid_data(".vox was not version 150"))
    } else {
        Ok(())
    }
}

fn invalid_data<E: Into<Box<dyn std::error::Error + Send + Sync>>>(msg: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

struct ChunkHeader {
    chunk_id: ChunkId,
    chunk_content_len: u32,
    chunk_children_len: u32,
}

/// A chunk in a MagicaVoxel `.vox` file
#[derive(Debug)]
pub struct Chunk {
    pub data: ChunkData,
    pub children: Vec<Chunk>,
}

#[derive(Debug)]
pub struct UnknownChunk {
    pub chunk_id: ChunkId,
    pub content: Vec<u8>,
}

#[derive(Debug)]
pub struct MatlChunk {
    material_id: i32,
    // TODO: even _type doesn't seem to always be present. What's a material with no type?
    material_type: Option<MaterialType>,
    // TODO: these don't always seem to be present; the docs on the format don't say anything about
    // when to expect them.
    /*
    weight: Option<f64>,
    rough: Option<f64>,
    spec: Option<f64>,
    ior: Option<f64>,
    att: Option<f64>,
    flux: Option<f64>,
    */
    extra: HashMap<String, String>,
}

#[derive(Debug)]
pub enum ChunkData {
    Main,
    Size {
        size_x: u32,
        size_y: u32,
        size_z: u32,
    },
    Xyzi {
        voxels: Vec<Voxel>,
    },
    Rgba {
        palette: Vec<Color>,
    },
    Matl(Box<MatlChunk>),
    Unknown(UnknownChunk),
}

#[derive(Debug)]
pub enum MaterialType {
    Diffuse,
    Metal,
    Glass,
    Emit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug)]
pub struct Voxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub color_index: u8,
}

/// The 4-byte chunk ID for a MagicaVoxel `.vox` file chunk.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ChunkId([u8; 4]);

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        let mut first = true;
        for b in self.0 {
            match first {
                true => first = false,
                false => write!(f, ", ")?,
            }
            match b {
                b'a'..=b'z' => write!(f, "b'{}'", char::from(b))?,
                b'A'..=b'Z' => write!(f, "b'{}'", char::from(b))?,
                _ => write!(f, "0x{:02x}", b)?,
            }
        }
        write!(f, "[")?;
        Ok(())
    }
}

impl PartialEq<[u8]> for ChunkId {
    fn eq(&self, other: &[u8]) -> bool {
        self.0 == other
    }
}

/// Read a chunk header
fn read_chunk_header<R: Read>(mut reader: R) -> io::Result<ChunkHeader> {
    let mut buf = [0u8; 12];
    reader.read_exact(&mut buf)?;
    let chunk_id = buf[..4]
        .try_into()
        .expect("slice should have been length 4");
    let chunk_content_len = u32::from_le_bytes(
        buf[4..8]
            .try_into()
            .expect("slice should have been length 4"),
    );
    let chunk_children_len = u32::from_le_bytes(
        buf[8..]
            .try_into()
            .expect("slice should have been length 4"),
    );

    Ok(ChunkHeader {
        chunk_id: ChunkId(chunk_id),
        chunk_content_len,
        chunk_children_len,
    })
}

/// Read & parse a chunk's main content:
fn parse_chunk<R: Read>(
    mut reader: R,
    chunk_id: ChunkId,
    chunk_content_len: u32,
) -> io::Result<Chunk> {
    let content_len = chunk_content_len
        .try_into()
        .expect("could not convert chunk content length into usize");
    let mut content = Vec::<u8>::with_capacity(content_len);
    content.resize(content_len, 0);
    reader.read_exact(&mut content)?;
    let chunk_data = match &chunk_id.0 {
        b"MAIN" => {
            if content.len() != 0 {
                return Err(invalid_data(
                    "MAIN chunk's content was non-zero; this chunk should have no content",
                ));
            }
            ChunkData::Main
        }
        b"SIZE" => {
            if content.len() != 12 {
                return Err(invalid_data("SIZE chunk's content was not 12 bytes"));
            }
            let size_x = u32::from_le_bytes(
                content[..4]
                    .try_into()
                    .expect("slice should have been length 4"),
            );
            let size_y = u32::from_le_bytes(
                content[4..8]
                    .try_into()
                    .expect("slice should have been length 4"),
            );
            let size_z = u32::from_le_bytes(
                content[8..]
                    .try_into()
                    .expect("slice should have been length 4"),
            );
            ChunkData::Size {
                size_x,
                size_y,
                size_z,
            }
        }
        b"XYZI" => {
            if content.len() < 4 {
                return Err(invalid_data("YXZI chunk's content was < 4 bytes"));
            }
            let n_voxels = u32::from_le_bytes(
                content[..4]
                    .try_into()
                    .expect("slice should have been length 4"),
            );
            if u64::from(n_voxels) * 4
                != u64::try_from(content.len() - 4)
                    .expect("content length should have fit in a u64")
            {
                return Err(invalid_data(
                    "YXZI chunk's numVoxels did not match the remaining data's size",
                ));
            }
            let mut voxels = Vec::new();
            for voxel_chunk in content[4..].chunks(4) {
                // These are backwards in the file, in IZYX order:
                let color_index = voxel_chunk[0];
                let z = voxel_chunk[1];
                let y = voxel_chunk[2];
                let x = voxel_chunk[3];
                voxels.push(Voxel {
                    x,
                    y,
                    z,
                    color_index,
                });
            }
            assert!(
                u64::try_from(voxels.len()).expect("voxels.len() should have fit in a u64")
                    == u64::from(n_voxels)
            );
            ChunkData::Xyzi { voxels }
        }
        b"RGBA" => {
            if content.len() != 4 * 256 {
                return Err(invalid_data("RGBA content was not 4 * 256 bytes"));
            }
            let mut palette = Vec::new();
            for rgba_chunk in content.chunks(4) {
                // These are backwards in the file, in ABGR order:
                let a = rgba_chunk[0];
                let b = rgba_chunk[1];
                let g = rgba_chunk[2];
                let r = rgba_chunk[3];
                let color = Color { r, b, g, a };
                palette.push(color);
            }
            assert!(palette.len() == 256);
            ChunkData::Rgba { palette }
        }
        b"MATL" => {
            println!("{:?}", content);
            let mut content_ptr = content.as_slice();
            let material_id = read_i32(&mut content_ptr)?;
            let mut dict = read_dict(&mut content_ptr)?;
            let material_type = {
                dict
                    .remove("_type")
                    .map(|material_type| {
                        match material_type.as_str() {
                            "_diffuse" => Ok(MaterialType::Diffuse),
                            "_metal" => Ok(MaterialType::Metal),
                            "_glass" => Ok(MaterialType::Glass),
                            "_emit" => Ok(MaterialType::Emit),
                            _ => Err(invalid_data(format!(
                                "MATL chunk's _type was {}",
                                material_type
                            )))
                        }
                    })
                    .transpose()?
            };
            /*
            let weight = dict
                .remove("_weight");
            let rough = dict
                .remove("_rough")
                .ok_or_else(|| invalid_data("MATL chunk DICT missing _rough"))?;
            let spec = dict
                .remove("_spec")
                .ok_or_else(|| invalid_data("MATL chunk DICT missing _spec"))?;
            ChunkData::Unknown(UnknownChunk { chunk_id, content })
            */
            ChunkData::Matl(Box::new(MatlChunk {
                material_id,
                material_type,
                extra: dict,
            }))
        }
        _ => ChunkData::Unknown(UnknownChunk { chunk_id, content }),
    };
    Ok(Chunk {
        data: chunk_data,
        children: Vec::new(),
    })
}

fn read_dict(mut read: impl Read) -> io::Result<HashMap<String, String>> {
    let kv_pairs = read_u32_as_usize(&mut read)?;
    let mut result = HashMap::new();
    println!("Will read {} pairs.", kv_pairs);
    for _ in 0..kv_pairs {
        let k = read_string(&mut read)?;
        let v = read_string(&mut read)?;
        println!("k = {:?}, v = {:?}.", k, v);
        result.insert(k, v);
    }
    Ok(result)
}

fn read_string(mut read: impl Read) -> io::Result<String> {
    println!("read_string");
    let buffer_len = read_u32_as_usize(&mut read)?;
    println!("done read size ({})", buffer_len);
    let mut data = Vec::with_capacity(buffer_len);
    data.resize(buffer_len, 0);
    println!("about to read_exact");
    read.read_exact(&mut data)?;
    println!("done");
    String::from_utf8(data).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn read_u32_as_usize(mut read: impl Read) -> io::Result<usize> {
    let mut buf = [0u8; 4];
    read.read_exact(&mut buf)?;
    let n = u32::from_le_bytes(buf);
    usize::try_from(n).map_err(|_| invalid_data("u32 value too big for usize"))
}

fn read_i32(mut read: impl Read) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    read.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    static LOGO: &[u8] = include_bytes!("vox/logo.vox");

    use super::from_reader;

    #[test]
    fn test_load_logo() {
        let logo = from_reader(std::io::Cursor::new(LOGO)).expect("logo.vox should parse");
        println!("Logo: {:#?}", logo);
    }

    #[test]
    fn test_show_sizes() {
        println!("ChunkData: {}B", std::mem::size_of::<super::ChunkData>());
        println!(
            "UnknownChunk: {}B",
            std::mem::size_of::<super::UnknownChunk>()
        );
    }
}
