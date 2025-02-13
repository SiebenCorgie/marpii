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
    let shader_crate_location = Path::new(shader_crate).canonicalize().unwrap();
    if !shader_crate_location.exists() {
        println!("cargo:warning=no crate at: {shader_crate_location:?}");
        return Err(SpirvBuilderError::CratePathDoesntExist(
            shader_crate_location,
        ));
    }

    println!("cargo:warning=Building shader {shader_crate_location:?}");

    let spirv_target_location = Path::new(destination_folder).canonicalize().unwrap();

    if !spirv_target_location.exists() {
        create_dir_all(&spirv_target_location).expect("Could not create spirv directory!");
    }

    let compiler_result = SpirvBuilder::new(&shader_crate_location, "spirv-unknown-vulkan1.2")
        .spirv_metadata(SpirvMetadata::Full)
        .print_metadata(MetadataPrintout::None)
        .capability(Capability::Int8)
        .capability(Capability::Int16)
        .capability(Capability::ImageQuery)
        .capability(Capability::RuntimeDescriptorArray)
        //.capability(Capability::GroupNonUniform)
        //.capability(Capability::InputAttachment)
        //.capability(Capability::InputAttachmentArrayDynamicIndexing)
        //.capability(Capability::InputAttachmentArrayNonUniformIndexing)
        .capability(Capability::RuntimeDescriptorArray)
        .capability(Capability::SampledImageArrayDynamicIndexing)
        .capability(Capability::SampledImageArrayNonUniformIndexing)
        .capability(Capability::ShaderNonUniform)
        .capability(Capability::StorageBufferArrayDynamicIndexing)
        .capability(Capability::StorageBufferArrayNonUniformIndexing)
        .capability(Capability::StorageImageArrayDynamicIndexing)
        .capability(Capability::StorageImageArrayNonUniformIndexing)
        .capability(Capability::StorageImageReadWithoutFormat)
        .capability(Capability::StorageImageWriteWithoutFormat)
        //.capability(Capability::UniformBufferArrayDynamicIndexing)
        //.capability(Capability::UniformBufferArrayNonUniformIndexing)
        .capability(Capability::VulkanMemoryModel)
        .build()?;

    println!("cargo:warning=Generated following Spirv entrypoints:");
    for e in &compiler_result.entry_points {
        println!("cargo:warning=    {e}");
    }
    let move_spirv_file = |spv_location: &Path, entry: Option<String>| {
        let mut target = spirv_target_location.clone();
        if let Some(e) = entry {
            target = target.join(format!("{output_name}_{e}.spv"));
        } else {
            target = target.join(format!("{output_name}.spv"));
        }

        println!("cargo:warning=Copying {spv_location:?} to {target:?}");
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

const RESDIR: &str = "../spirv";

fn clean_up() {
    let path = PathBuf::from(RESDIR);
    if path.exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }

    std::fs::create_dir(&path).unwrap();
}

// Builds rust shader crate and all glsl shaders.
fn main() {
    println!("cargo:rerun-if-changed=../iced-marpii-shared/src/lib.rs");
    println!("cargo:rerun-if-changed=../shader-quad/src/lib.rs");
    println!("cargo:rerun-if-changed=../shader-mesh/src/lib.rs");
    println!("cargo:rerun-if-changed=../shader-text/src/lib.rs");

    //cleanup resource dir
    clean_up();

    //build shader crate. generates a module per entry point
    compile_rust_shader("shader-quad", "../shader-quad/", RESDIR).unwrap();
    compile_rust_shader("shader-mesh", "../shader-mesh/", RESDIR).unwrap();
    compile_rust_shader("shader-text", "../shader-text/", RESDIR).unwrap();
}
