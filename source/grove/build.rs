use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let repository_dir = crate_dir.join("../..").canonicalize().expect("Grove belongs to the workspace");
    let material_dir = repository_dir.join("script/material/node_modules/material-icon-theme");
    let icon_dir = material_dir.join("icons");
    let manifest_path = material_dir.join("dist/material-icons.json");

    println!("cargo:rerun-if-changed={}", repository_dir.join("script/material/package.json").display());
    println!("cargo:rerun-if-changed={}", repository_dir.join("script/material/prepare.ts").display());
    println!("cargo:rerun-if-changed={}", material_dir.display());
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    if !icon_dir.is_dir() || !manifest_path.is_file() {
        panic!(
            "Material Icon Theme assets are not prepared; run `bash \"{}\" prepare` before building Grove",
            repository_dir.join("script/grove.sh").display()
        );
    }

    println!("cargo:rustc-env=GROVE_MATERIAL_ICON_DIR={}", icon_dir.display());
    println!("cargo:rustc-env=GROVE_MATERIAL_MANIFEST_PATH={}", manifest_path.display());
}
