/// Load MagicaVoxel files
pub mod io;

use io::{Chunk, ChunkData, Color, Voxel};

/// Get the voxel data from the loaded Magica file.
fn find_xyzi_data(top_chunk: &Chunk) -> anyhow::Result<&[Voxel]> {
    if !matches!(top_chunk.data, ChunkData::Main) {
        anyhow::bail!("top-level chunk was not the main chunk?");
    }
    let mut xyzi_voxels = None;
    for child in top_chunk.children.iter() {
        if let ChunkData::Xyzi { voxels } = &child.data {
            if xyzi_voxels.is_some() {
                anyhow::bail!("Multiple XYZI chunks in model?");
            }
            xyzi_voxels = Some(voxels.as_slice());
        }
    }
    xyzi_voxels.ok_or_else(|| anyhow::anyhow!("no XYZI chunk in model"))
}

/// Get the voxel data from the loaded Magica file.
fn find_rgba_data(top_chunk: &Chunk) -> anyhow::Result<&[Color]> {
    if !matches!(top_chunk.data, ChunkData::Main) {
        anyhow::bail!("top-level chunk was not the main chunk?");
    }
    let mut palette = None;
    for child in top_chunk.children.iter() {
        if let ChunkData::Rgba { palette: pal } = &child.data {
            if palette.is_some() {
                anyhow::bail!("Multiple RGBA chunks in model?");
            }
            palette = Some(pal.as_slice());
        }
    }
    palette.ok_or_else(|| anyhow::anyhow!("no RGBA chunk in model"))
}
