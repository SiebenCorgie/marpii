use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use spirv_builder::{
    Capability, MetadataPrintout, ModuleResult, SpirvBuilder, SpirvBuilderError, SpirvMetadata,
};

///Builds the shader crate and moves all files to a location that can be found by the renderer's loader.
pub fn compile_rust_shader(
    output_name: &str,
    shader_crate: &str,
    destination_folder: &str,
) -> Result<(), SpirvBuilderError> {
    println!("compile shader crate: {}", shader_crate);

    let shader_crate_location = Path::new(shader_crate).canonicalize().unwrap();
    if !shader_crate_location.exists() {
        println!("cargo:warning=no crate at: {:?}", shader_crate_location);
        return Err(SpirvBuilderError::CratePathDoesntExist(
            shader_crate_location,
        ));
    }

    println!("cargo:warning=Building shader {:?}", shader_crate_location);

    let spirv_target_location = Path::new(destination_folder).canonicalize().unwrap();

    if !spirv_target_location.exists() {
        create_dir_all(&spirv_target_location).expect("Could not create spirv directory!");
    }

    let compiler_result = SpirvBuilder::new(&shader_crate_location, "spirv-unknown-vulkan1.2")
        .spirv_metadata(SpirvMetadata::NameVariables)
        .print_metadata(MetadataPrintout::None)
        .capability(Capability::Int8)
        .capability(Capability::Int16)
        .capability(Capability::ImageQuery)
        .capability(Capability::RuntimeDescriptorArray)
        .capability(Capability::StorageImageReadWithoutFormat)
        .capability(Capability::StorageImageWriteWithoutFormat)
        .build()?;

    println!("cargo:warning=Generated following Spirv entrypoints:");
    for e in &compiler_result.entry_points {
        println!("cargo:warning=    {}", e);
    }

    let move_spirv_file = |spv_location: &Path, entry: Option<String>| {
        let mut target = spirv_target_location.clone();
        if let Some(e) = entry {
            target = target.join(&format!("{}_{}.spv", output_name, e));
        } else {
            target = target.join(&format!("{}.spv", output_name));
        }

        println!("cargo:warning=Copying {:?} to {:?}", spv_location, target);
        std::fs::copy(spv_location, &target).expect("Failed to copy spirv file!");
    };

    match compiler_result.module {
        ModuleResult::MultiModule(modules) => {
            //Note currently ignoring entry name since all of them should be "main", just copying the
            //shader files. Later might use a more sophisticated approach.
            for (entry, src_file) in modules {
                move_spirv_file(&src_file, Some(entry));
            }
        }
        ModuleResult::SingleModule(path) => {
            move_spirv_file(&path, None);
        }
    };
    Ok(())
}

#[allow(dead_code)]
fn build_glsl(path: &str, name: &str) {
    //TODO: build all files that do not end with ".glsl". and copy to
    // RESDIR as well.

    let src = PathBuf::from(path);
    if !src.exists(){
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
        .arg("-o")
        .arg(target)
        .output()
        .unwrap();

    if !command.status.success() {
        println!("cargo:warning=Out: {:#?}", std::str::from_utf8(&command.stdout).unwrap());
        println!("cargo:warning=Err: {:#?}", std::str::from_utf8(&command.stderr).unwrap());
    }
}

const RESDIR: &str = "../resources";

fn clean_up() {
    let path = PathBuf::from(RESDIR);
    if path.exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }

    std::fs::create_dir(&path).unwrap();
}

// Builds rust shader crate and all glsl shaders.
fn main() {
    println!("cargo:rerun-if-changed=../marpii-rmg-task-shader");
    println!("cargo:rerun-if-changed=../resources");

    //cleanup resource dir
    clean_up();

    //build shader crate. generates a module per entry point
    compile_rust_shader("rshader", "../marpii-rmg-task-shader/", RESDIR).unwrap();

    //NOTE: keeping it around for compatibility
    build_glsl("../marpii-rmg-task-shader/glsl/egui.vert", "eguivert.spv");
    build_glsl("../marpii-rmg-task-shader/glsl/egui.frag", "eguifrag.spv");
}
