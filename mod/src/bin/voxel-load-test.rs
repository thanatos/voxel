use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

fn main() {
    let args = Args::parse();

    let module = voxel_mod::Module::load_from_path(args.path).unwrap();
    println!("{:#?}", module);
}
