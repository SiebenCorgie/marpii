use std::path::PathBuf;
fn build_glsl(path: &str, name: &str, entry_point: &str) {
    //TODO: build all files that do not end with ".glsl". and copy to
    // RESDIR as well.

    let src = PathBuf::from(path);
    if !src.exists() {
        println!("cargo:warning=Shader does not exist at {:?}", src);
        return;
    }
    let target = PathBuf::from(RESDIR).join(name);
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }

    let command = std::process::Command::new("glslangValidator")
        .arg("-g")
        .arg("-V")
        .arg(path)
        .arg("--target-env")
        .arg("vulkan1.3")
        .arg("-e")
        .arg("main")
        .arg("--source-entrypoint")
        .arg(entry_point)
        .arg("-o")
        .arg(target)
        .output()
        .unwrap();

    if !command.status.success() {
        println!(
            "cargo:warning=Out: {:#?}",
            std::str::from_utf8(&command.stdout).unwrap()
        );
        println!(
            "cargo:warning=Err: {:#?}",
            std::str::from_utf8(&command.stderr).unwrap()
        );
    }
}

const RESDIR: &str = "resources/";

pub fn ensure_res() {
    if !PathBuf::from(RESDIR).exists() {
        std::fs::create_dir_all(RESDIR).unwrap();
    }
}

// Builds rust shader crate and all glsl shaders.
fn main() {
    println!("cargo:rerun-if-changed=triangle_shader.comp");
    ensure_res();
    build_glsl("triangle_shader.comp", "triangle_shader.spv", "main");
}
