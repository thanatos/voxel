use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::text_rendering::freetype::{FtFace, FtLibrary};
use crate::text_rendering::cache::GlyphCache;

pub struct Fonts {
    pub deja_vu: FtFace,
    pub deja_vu_cache: GlyphCache,
    pub press_start_2p: FtFace,
}

impl Fonts {
    pub fn init(in_bench: bool) -> anyhow::Result<Fonts> {
        let freetype_lib = Arc::new(Mutex::new(FtLibrary::new()?));
        let third_party = {
            let mut resources_path = determine_resources_path(in_bench)?;
            resources_path.push("third-party");
            resources_path
        };
        let press_start_2p = {
            let mut p = third_party.to_owned();
            p.push("press-start-2p");
            p.push("PressStart2P.ttf");
            load_font(freetype_lib.clone(), &p)?
        };
        let mut deja_vu = {
            let mut p = third_party.to_owned();
            p.push("deja-vu");
            p.push("dejavu-fonts-ttf-2.37");
            p.push("ttf");
            p.push("DejaVuSansMono.ttf");
            load_font(freetype_lib.clone(), &p)?
        };

        let deja_vu_cache = GlyphCache::new(&mut deja_vu, 14 << 6)?;

        Ok(Fonts {
            deja_vu,
            deja_vu_cache,
            press_start_2p,
        })
    }
}

// FIXME: this is a giant hack.
fn determine_resources_path(in_bench: bool) -> anyhow::Result<PathBuf> {
    let mut path = std::env::current_exe()?;
    path.pop(); // remove the exe filename
    if in_bench {
        path.pop(); // remove deps
    }
    path.pop(); // remove debug/release
    path.pop(); // remove target
    Ok(path)
}

fn load_font<P: AsRef<Path>>(ft_lib: Arc<Mutex<FtLibrary>>, path: P) -> anyhow::Result<FtFace> {
    let data = std::fs::read(path)?;
    Ok(FtFace::new_from_buffer(ft_lib, data.into_boxed_slice())?)
}
