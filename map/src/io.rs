use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};

use serde::Serialize;

use voxel_mod::ModuleBlockDefinition;

use crate::octree::{LocationCode, OctreeNode, SubCube};
use crate::Chunk;

// Below, we construct a map from block definitions, to the ID we will give that type of block in
// the encoded chunk. This wrapper does Eq & Hash on the address/pointer of the reference to that
// block definition, not on the block definition itself (which doesn't implement either Eq or Hash,
// and would likely be far more expensive). Since the idea is that we store block defs. only once,
// and use Arcs to refer to them, the same block def. should always have the same memory address
// (at least, for the duration of encoding the chunk; we only use this to build the definition → ID
// mapping. Once that's build, the final palette records the module's string ID & the block within
// that module's string ID.).
#[derive(Debug)]
struct HashableRef<'a, T>(Option<&'a T>);

impl<T> PartialEq for HashableRef<'_, T> {
    fn eq(&self, other: &HashableRef<'_, T>) -> bool {
        match (self.0, other.0) {
            (None, None) => true,
            (Some(s), Some(o)) => std::ptr::eq(s, o),
            _ => false,
        }
    }
}

impl<T> Eq for HashableRef<'_, T> {}

impl<T> Hash for HashableRef<'_, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            None => {
                Hash::hash(&0u8, state);
            }
            Some(r) => {
                Hash::hash(&1u8, state);
                std::ptr::hash(*r, state);
            }
        }
    }
}

#[derive(Serialize)]
struct ChunkOnDisk<'a> {
    palette: Vec<Option<(String, &'a str)>>,
    #[serde(with = "serde_bytes")]
    blocks: Vec<u8>,
}

fn write_chunk_octree(chunk: &Chunk) -> ChunkOnDisk {
    // Iterate through the blocks in the chunk, and assign integer IDs to the various types of
    // blocks in this chunk.
    let mut palette = Vec::<Option<&ModuleBlockDefinition>>::new();
    let mut block_ids = HashMap::<HashableRef<ModuleBlockDefinition>, u32>::new();

    for (_, block) in chunk.blocks() {
        let block_ref = block.as_ref().map(|bdref| bdref.as_ref());
        block_ids.entry(HashableRef(block_ref)).or_insert_with(|| {
            let this_id = palette.len();
            palette.push(block_ref);
            u32::try_from(this_id)
                .expect("there should never be more than 2 ** 32 blocks in a chunk")
        });
    }

    let palette = palette
        .into_iter()
        .map(|mdb| mdb.map(|b| (b.module().id().to_owned(), b.id())))
        .collect::<Vec<_>>();

    /*
     * The entire chunk is serialized as a CBOR map:
     * ```
     * {
     *     "palette": <block assignment table>,
     *     "blocks": <chunk data>,
     * }
     * ```
     */

    // Encode the octree:

    let mut blocks = Vec::new();
    let mut current_location = LocationCode::ROOT;
    'outer: loop {
        let volume = chunk.get_octree().get_volume(current_location).unwrap();
        match volume {
            OctreeNode::Present(vd) => {
                let block_id = block_ids
                    .get(&HashableRef(vd.0.as_ref().map(|arc| arc.as_ref())))
                    .unwrap();
                blocks.push(0);
                write_varint(&mut blocks, *block_id).expect("vector writes cannot fail");
                loop {
                    let (parent, sub_cube) = match current_location.sub_cube() {
                        Some(t) => t,
                        None => break 'outer, // we're at the root
                    };
                    match sub_cube.next_sibling() {
                        Some(sibling) => {
                            current_location = parent.push_sub_cube(sibling);
                            break;
                        }
                        None => {
                            current_location = parent;
                            // Go up; the next loop iteration will figure out the sibling at the
                            // parent's level.
                        }
                    }
                }
            }
            OctreeNode::Subdivided => {
                blocks.push(1);
                current_location = current_location.push_sub_cube(SubCube::LowerSw);
            }
        }
    }

    ChunkOnDisk { palette, blocks }
}

/// Write a varint; this is not a CBOR varint, this is just used for encoding block IDs in the
/// encoded octree. The varint is encoded as least-significant bits first (so, sort of
/// little-endian), with the most-significant bit of each byte reserved: it is set if more bytes
/// follow. Each byte thus carries 7 bits, of increasing significance. This function only handles
/// u32s, as that's all the octree needs.
fn write_varint<W: Write>(mut write: W, n: u32) -> io::Result<()> {
    if n < 0b0111_1111
    /* 7 bits */
    {
        write.write_all(&[u8::try_from(n).unwrap()])
    } else if n < 0b0011_1111_1111_1111
    /* 14 bits */
    {
        write.write_all(&[
            u8::try_from(n >> 7).unwrap() | 0x80,
            u8::try_from(n & 0x7f).unwrap(),
        ])
    } else if n < 0b0001_1111_1111_1111_1111_1111
    /* 21 bits */
    {
        write.write_all(&[
            u8::try_from(n >> 14).unwrap() | 0x80,
            u8::try_from((n >> 7) & 0x7f).unwrap() | 0x80,
            u8::try_from(n & 0x7f).unwrap(),
        ])
    } else if n < 0b1111_1111_1111_1111_1111_1111_1111
    /* 28 bits */
    {
        write.write_all(&[
            u8::try_from(n >> 21).unwrap() | 0x80,
            u8::try_from((n >> 14) & 0x7f).unwrap() | 0x80,
            u8::try_from((n >> 7) & 0x7f).unwrap() | 0x80,
            u8::try_from(n & 0x7f).unwrap(),
        ])
    } else {
        write.write_all(&[
            u8::try_from(n >> 28).unwrap() | 0x80,
            u8::try_from((n >> 21) & 0x7f).unwrap() | 0x80,
            u8::try_from((n >> 14) & 0x7f).unwrap() | 0x80,
            u8::try_from((n >> 7) & 0x7f).unwrap() | 0x80,
            u8::try_from(n & 0x7f).unwrap(),
        ])
    }
}

/*
/// Write a CBOR string
fn cbor_write_string<W: Write>(w: W, s: &str) -> io::Result<()> {
    // Strings are major type 3, the associated data is the length:
    w.write(&[(3 << 5) | u8::try_from("palette".len())
}
*/

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use voxel_mod::Module;

    use crate::{Chunk, ChunkRelativeCoord};

    static MINIMAL_MOD_BLOCK_DEFS: &str = r#"
dirt:
  texture: dirt.png
  color:
    r: 143
    g: 86
    b: 59
  homogeneous: true
"#;

    fn minimal_mod() -> Arc<Module> {
        let block_defs = voxel_mod::block_defs::load_block_definitions(
            (&MINIMAL_MOD_BLOCK_DEFS[1..]).as_bytes(),
        )
        .unwrap();
        Module::new(
            "test".to_owned(),
            "[test]".to_owned(),
            PathBuf::from("[internal]"),
            block_defs,
        )
    }

    #[test]
    fn a_test() {
        let minimal_mod = minimal_mod();
        let dirt = minimal_mod.block_by_id("dirt").unwrap();
        let mut chunk = Chunk::new();
        chunk.set_block(ChunkRelativeCoord::new(0, 0, 0), Some(dirt));

        println!("Octree: {:#?}", chunk.get_octree());

        let chunk_on_disk = super::write_chunk_octree(&chunk);
        let buffer = serde_cbor::to_vec(&chunk_on_disk).expect("failed to serialize to CBOR");
        println!("({}B) {:?}", buffer.len(), buffer);

        let brotbuf = {
            use std::io::Write;
            let mut writer = brotli2::write::BrotliEncoder::new(Vec::<u8>::new(), 11);
            writer.write_all(&buffer).unwrap();
            writer.finish().unwrap()
        };
        println!("With brotli: ({}B) {:?}", brotbuf.len(), brotbuf);

        // This is the expected value of the above write.
    }
}
