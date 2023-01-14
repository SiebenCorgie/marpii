use std::{fs::create_dir_all, path::PathBuf};

#[allow(dead_code)]
fn build_glsl(path: &str, name: &str, target: &str) {
    //TODO: build all files that do not end with ".glsl". and copy to
    // RESDIR as well.
    let target_path = PathBuf::from(target).join(name);
    if target_path.exists() {
        std::fs::remove_file(&target_path).unwrap();
    }

    let command = std::process::Command::new("glslangValidator")
        .arg("-g")
        .arg("-V")
        .arg(path)
        .arg("-o")
        .arg(target_path)
        .output()
        .unwrap();

    if !command.status.success() {
        println!(
            "cargo:warning=Out: {}",
            std::str::from_utf8(&command.stdout).unwrap()
        );
        println!(
            "cargo:warning=Err: {}",
            std::str::from_utf8(&command.stderr).unwrap()
        );
    }
}

const RESDIR: &str = "resources/";

pub fn ensure_res() {
    if !PathBuf::from(RESDIR).exists() {
        create_dir_all(RESDIR).unwrap();
    }
}

// Builds rust shader crate and all glsl shaders.
fn main() {
    println!("cargo:rerun-if-changed=glsl/");
    ensure_res();
    build_glsl("glsl/simulation.comp", "simulation.spv", RESDIR);
    build_glsl("glsl/forward.vert", "forward_vs.spv", RESDIR);
    build_glsl("glsl/forward.frag", "forward_fs.spv", RESDIR);
}
