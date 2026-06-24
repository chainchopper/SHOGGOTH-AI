// SPDX-License-Identifier: Apache-2.0
/// Integration tests for the agentic workload parser and routing engine.
///
/// Verifies all 15+ keyword patterns across 4 workload types
/// route to the correct hardware targets.

use shoggoth_agent::parser::{ShoggothParser, WorkloadType};
use shoggoth_agent::templates::{self, TemplateType};
use shoggoth_agent::ShoggothAgent;

// ── Workload Classification ────────────────────────────────────────────────────

#[test]
fn test_classify_pytorch_as_tensor_compute() {
    let parser = ShoggothParser::new();
    let samples = [
        "import torch.nn as nn",
        "model = nn.Linear(20, 20).cuda()",
        "import torch; tensor.cuda()",
        "from torch.nn import Transformer",
        "torch.nn.functional.relu(x)",
    ];
    for s in &samples {
        let (wl, _) = parser.analyze_source_code(s);
        assert_eq!(wl, WorkloadType::TensorCompute, "Failed for: {s}");
    }
}

#[test]
fn test_classify_alphafold_ml() {
    let parser = ShoggothParser::new();
    let samples = [
        "from alphafold import model",
        "import esm; model = esm.ESM2()",
        "unslothFastLanguageModel.from_pretrained",
        "gguf quantize model.gguf",
        "safetensors.torch.load_file",
    ];
    for s in &samples {
        let (wl, _) = parser.analyze_source_code(s);
        assert_eq!(wl, WorkloadType::TensorCompute, "Failed for: {s}");
    }
}

#[test]
fn test_classify_ray_tracing() {
    let parser = ShoggothParser::new();
    let samples = [
        "// Unreal Engine 5 Nanite",
        "Omniverse Kit extension",
        "BakeRayTracing for GI",
        "BVH traversal shader",
        "OptiX denoiser",
        "PathTracing integrator",
    ];
    for s in &samples {
        let (wl, _) = parser.analyze_source_code(s);
        assert_eq!(wl, WorkloadType::RayTracing, "Failed for: {s}");
    }
}

#[test]
fn test_classify_raster_graphics() {
    let parser = ShoggothParser::new();
    let samples = [
        "using UnityEngine;",
        "Blender Python script",
        "compositor.render()",
        "viewport.update()",
    ];
    for s in &samples {
        let (wl, _) = parser.analyze_source_code(s);
        assert_eq!(wl, WorkloadType::RasterGraphics, "Failed for: {s}");
    }
}

#[test]
fn test_classify_generic_as_cpu() {
    let parser = ShoggothParser::new();
    let (wl, target) = parser.analyze_source_code("console.log('hello')");
    assert_eq!(wl, WorkloadType::GeneralCPU);
    assert!(target.node_friendly_name.contains("Xeon"));
}

#[test]
fn test_classify_case_insensitive() {
    let parser = ShoggothParser::new();
    let (wl, _) = parser.analyze_source_code("TORCH.NN.Linear(20, 20).CUDA()");
    assert_eq!(wl, WorkloadType::TensorCompute);
}

// ── Template Selection ─────────────────────────────────────────────────────────

#[test]
fn test_template_routing_ray_tracing() {
    assert_eq!(
        templates::select_template(&WorkloadType::RayTracing),
        TemplateType::RenderFarm
    );
}

#[test]
fn test_template_routing_tensor_compute() {
    assert_eq!(
        templates::select_template(&WorkloadType::TensorCompute),
        TemplateType::HeavyCompute
    );
}

#[test]
fn test_template_routing_raster() {
    assert_eq!(
        templates::select_template(&WorkloadType::RasterGraphics),
        TemplateType::AsyncGameRuntime
    );
}

#[test]
fn test_all_templates_generate_valid_toml() {
    for template in &[
        TemplateType::RenderFarm,
        TemplateType::HeavyCompute,
        TemplateType::AsyncGameRuntime,
        TemplateType::GenomicProcessing,
        TemplateType::Generic,
    ] {
        let manifest = template.generate_manifest("test-project");
        assert!(!manifest.is_empty());
        assert!(manifest.contains("[workload]"));
        assert!(manifest.contains("test-project"));
    }
}

// ── Full Agent Pipeline ────────────────────────────────────────────────────────

#[test]
fn test_agent_analyze_and_route_pytorch() {
    let agent = ShoggothAgent::new();
    let decision = agent.analyze_and_route("import torch.nn as nn");
    assert_eq!(decision.workload, WorkloadType::TensorCompute);
    assert!(decision.confidence > 0.8);
    assert!(!decision.target.node_friendly_name.is_empty());
    assert!(!decision.target.primary_reason.is_empty());
}

#[test]
fn test_agent_analyze_generic() {
    let agent = ShoggothAgent::new();
    let decision = agent.analyze_and_route("echo hello");
    assert_eq!(decision.workload, WorkloadType::GeneralCPU);
    assert!(decision.target.node_friendly_name.contains("Xeon"));
}

// ── Template Manifest Integrity ─────────────────────────────────────────────────

#[test]
fn test_render_farm_template_has_rt_cores() {
    let manifest = TemplateType::RenderFarm.generate_manifest("blender");
    assert!(manifest.contains("nvidia-rtx-5090"));
    assert!(manifest.contains("bc250-apu-grid"));
    assert!(manifest.contains("ray_tracing"));
}

#[test]
fn test_heavy_compute_template_has_pipeline_parallelism() {
    let manifest = TemplateType::HeavyCompute.generate_manifest("pytorch");
    assert!(manifest.contains("pipeline_parallelism"));
    assert!(manifest.contains("activation_only"));
    assert!(manifest.contains("amd-mi50-cluster"));
}

#[test]
fn test_genomic_template_has_scylla_keyspace() {
    let manifest = TemplateType::GenomicProcessing.generate_manifest("genex");
    assert!(manifest.contains("scylla_keyspace"));
    assert!(manifest.contains("fasta_parsing"));
}
