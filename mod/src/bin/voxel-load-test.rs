use std::path::PathBuf;

use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
    path: PathBuf,
}

fn main() {
    let args = Args::from_args();

    let module = voxel_mod::Module::load_from_path(args.path).unwrap();
    println!("{:#?}", module);
}
