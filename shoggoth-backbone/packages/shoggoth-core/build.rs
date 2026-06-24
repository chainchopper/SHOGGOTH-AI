// SPDX-License-Identifier: Apache-2.0
// build.rs — Compile GLSL compute shaders to SPIR-V at build time.
//
// Uses shaderc-rs to compile all .glsl files in src/shaders/ into SPIR-V
// binaries. The compiled shaders are embedded in the binary via
// include_bytes!() so no external files are needed at runtime.

use std::path::PathBuf;

fn main() {
    // Only compile shaders when the shaderc feature is available and
    // we're on a platform that supports Vulkan/SPIR-V.
    println!("cargo:rerun-if-changed=src/shaders/");

    let shader_dir = PathBuf::from("src/shaders");
    if !shader_dir.exists() {
        println!("cargo:warning=Shader directory not found at {:?}; skipping shader compilation", shader_dir);
        return;
    }

    let mut compiler = shaderc::Compiler::new().expect("Failed to create shaderc compiler");
    let options = shaderc::CompileOptions::new()
        .expect("Failed to create shaderc compile options");

    // Add include directory for shared shader headers.
    let mut options = options;
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_6);
    options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    options.set_generate_debug_info();

    for entry in std::fs::read_dir(&shader_dir).expect("Failed to read shader directory") {
        let entry = entry.expect("Failed to read shader entry");
        let path = entry.path();

        if path.extension().map_or(true, |ext| ext != "glsl" && ext != "comp") {
            continue;
        }

        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read shader {:?}: {e}", path));

        let shader_kind = determine_shader_kind(&path);

        println!("cargo:warning=Compiling shader: {:?}", path.file_name().unwrap());

        let binary_result = compiler.compile_into_spirv(
            &source,
            shader_kind,
            path.file_name().unwrap().to_str().unwrap(),
            "main",
            Some(&options),
        ).expect(&format!("Failed to compile shader {:?}", path));

        // Write the compiled SPIR-V binary to the output directory.
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let mut out_path = out_dir.clone();
        out_path.push(path.file_stem().unwrap());
        out_path.set_extension("spv");

        std::fs::write(&out_path, binary_result.as_binary_u8())
            .unwrap_or_else(|e| panic!("Failed to write SPIR-V binary {:?}: {e}", out_path));

        println!("cargo:warning=  -> {:?}", out_path);
    }
}

fn determine_shader_kind(path: &PathBuf) -> shaderc::ShaderKind {
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if file_name.contains("comp") || file_name.contains("compute") {
        shaderc::ShaderKind::Compute
    } else if file_name.contains("vert") {
        shaderc::ShaderKind::Vertex
    } else if file_name.contains("frag") {
        shaderc::ShaderKind::Fragment
    } else {
        // Default to compute for generic .glsl files in the shaders directory.
        shaderc::ShaderKind::Compute
    }
}
