use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::text_rendering::freetype::{FtFace, FtLibrary};

pub struct Fonts {
    pub deja_vu: FtFace,
    pub press_start_2p: FtFace,
}

impl Fonts {
    pub fn init() -> anyhow::Result<Fonts> {
        let freetype_lib = Arc::new(FtLibrary::new()?);
        let third_party = {
            let mut resources_path = determine_resources_path()?;
            resources_path.push("third-party");
            resources_path
        };
        let press_start_2p = {
            let mut p = third_party.to_owned();
            p.push("press-start-2p");
            p.push("PressStart2P.ttf");
            load_font(freetype_lib.clone(), &p)?
        };
        let deja_vu = {
            let mut p = third_party.to_owned();
            p.push("deja-vu");
            p.push("dejavu-fonts-ttf-2.37");
            p.push("ttf");
            p.push("DejaVuSansMono.ttf");
            load_font(freetype_lib.clone(), &p)?
        };
        Ok(Fonts {
            deja_vu,
            press_start_2p,
        })
    }
}

// FIXME: this is a giant hack.
fn determine_resources_path() -> anyhow::Result<PathBuf> {
    let mut path = std::env::current_exe()?;
    path.pop(); // remove the exe filename
    path.pop(); // remove debug/release
    path.pop(); // remove target
    Ok(path)
}

fn load_font<P: AsRef<Path>>(ft_lib: Arc<FtLibrary>, path: P) -> anyhow::Result<FtFace> {
    let data = std::fs::read(path)?;
    Ok(FtFace::new_from_buffer(ft_lib, data.into_boxed_slice())?)
}
