//!simple shader building utility. Usually hooked into the build script of the examples.

use std::{fs::create_dir_all, path::{Path, PathBuf}, io::{Read, Write}};

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
        println!("no crate at: {:?}", shader_crate_location);
        return Err(SpirvBuilderError::CratePathDoesntExist(
            shader_crate_location.to_owned(),
        ));
    }

    println!("Building shader {:?}", shader_crate_location);

    let spirv_target_location = Path::new(destination_folder).canonicalize().unwrap();

    if !spirv_target_location.exists() {
        println!("{:?} does not exist, creating...", spirv_target_location);
        create_dir_all(&spirv_target_location).expect("Could not create spirv directory!");
    }

    println!("SpirV dir @ {:?}", spirv_target_location);

    let compiler_result = SpirvBuilder::new(&shader_crate_location, "spirv-unknown-vulkan1.2")
        .spirv_metadata(SpirvMetadata::NameVariables)
        .print_metadata(MetadataPrintout::None)
        .capability(Capability::Int8)
        .capability(Capability::Int16)
        .capability(Capability::ImageQuery)
        .capability(Capability::RuntimeDescriptorArray)
        .build()?;

    println!("Generated following Spirv entrypoints:");
    for e in &compiler_result.entry_points {
        println!("{}", e);
    }

    let move_spirv_file = |spv_location: &Path, entry: Option<String>| {
        let mut target = spirv_target_location.clone();
        if let Some(e) = entry {
            target = target.join(&format!("{}_{}.spv", output_name, e));
        } else {
            target = target.join(&format!("{}.spv", output_name));
        }

        println!("Copying {:?} to {:?}", spv_location, target);
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

fn build_glsl(path: &str, target: &str) {

    if PathBuf::from(target).exists(){
        std::fs::remove_file(target).unwrap();
    }

    let command = std::process::Command::new("glslangValidator")
        .arg("-V")
        .arg(path)
        .arg("-o")
        .arg(target)
        .output()
        .unwrap();

    if !command.status.success(){
        println!("Out: {}", std::str::from_utf8(&command.stdout).unwrap());
        println!("Err: {}", std::str::from_utf8(&command.stderr).unwrap());
    }
}

fn main() {
    compile_rust_shader("test_shader", "examples/rust_shader", "resources/")
        .expect("Failed to build shader");

    compile_rust_shader(
        "vertex_graphics_shader",
        "examples/vertex_graphics_shader",
        "resources/",
    )
    .expect("Failed to build shader");


    compile_rust_shader(
        "rmg_shader",
        "examples/rmg_shader",
        "resources/",
    )
    .expect("Failed to build shader");

    build_glsl(
        "examples/rmg_shader/glsl/simulation.comp",
        "resources/simulation.spv",
    );
}
