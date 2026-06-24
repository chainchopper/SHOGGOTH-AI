// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-agent/src/parser.rs — AST-based workload classifier.
//
// Scans raw source code (Python, C++, GLSL, HLSL, Rust) for keyword
// signatures that indicate heavy compute/graphics workloads. Maps each
// pattern to a workload type and a suggested hardware target.
//
// In production, this integrates with tree-sitter for proper AST parsing
// across languages. For now, it uses high-signal keyword matching.

use std::collections::HashMap;

// ── Types ──────────────────────────────────────────────────────────────────────

/// Classification of a detected workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadType {
    /// PyTorch, AlphaFold, CUDA tensor operations.
    TensorCompute,
    /// Unreal Engine, Omniverse, Blender ray-tracing.
    RayTracing,
    /// Legacy Unity, client UI, raster workloads.
    RasterGraphics,
    /// Heavy compilation, scripting, file I/O.
    GeneralCPU,
}

/// A suggested hardware target from static analysis.
#[derive(Debug, Clone)]
pub struct HardwareTarget {
    pub node_friendly_name: String,
    pub primary_reason: String,
}

// ── Parser ─────────────────────────────────────────────────────────────────────

/// Maintains a catalog of hardware pools and maps code signatures to them.
#[derive(Debug)]
pub struct ShoggothParser {
    /// Hardware inventory: pool name → list of node friendly names.
    hardware_inventory: HashMap<String, Vec<String>>,
}

impl ShoggothParser {
    /// Creates a new parser with the lab hardware inventory.
    #[must_use]
    pub fn new() -> Self {
        let mut inventory = HashMap::new();

        inventory.insert(
            "RTX_LEAD".into(),
            vec!["RTX 5090 (Edge)".into(), "RTX 4090 (Edge)".into()],
        );
        inventory.insert(
            "AMD_INSTINCT".into(),
            vec!["MI50 Instinct #1".into(), "MI50 Instinct #2".into()],
        );
        inventory.insert(
            "APU_GRID".into(),
            (1..=12)
                .map(|i| format!("BC250 Node {i:02}"))
                .collect(),
        );
        inventory.insert(
            "XEON_BRAIN".into(),
            vec!["Dual Xeon 6240 (512 GB RAM)".into()],
        );
        inventory.insert(
            "NVIDIA_LEGACY".into(),
            vec!["RTX 3090 (Compute)".into()],
        );
        inventory.insert(
            "AMD_ENTERPRISE".into(),
            vec!["AMD V620 (Enterprise)".into()],
        );

        Self {
            hardware_inventory: inventory,
        }
    }

    /// Scans source code and returns a (WorkloadType, HardwareTarget) pair.
    ///
    /// Matching priority (first match wins):
    ///   1. Tensor/ML frameworks → AMD MI50 / BC250 grid.
    ///   2. Ray-tracing engines → RTX 5090/4090.
    ///   3. Raster/UI → BC250 APU grid.
    ///   4. Default fallback → Xeon host.
    #[must_use]
    pub fn analyze_source_code(&self, source_code: &str) -> (WorkloadType, HardwareTarget) {
        let lower = source_code.to_lowercase();

        // ── Tensor Compute Patterns ──
        if lower.contains("torch.nn")
            || lower.contains("torch.cuda")
            || lower.contains("alphafold")
            || lower.contains("cuda(")
            || lower.contains("tensorflow")
            || lower.contains("jax.numpy")
            || lower.contains("unsloth")
            || lower.contains("gguf")
            || lower.contains("safetensors")
            || lower.contains("esm_")
            || lower.contains("protein")
            || lower.contains("nucleotide_transformer")
        {
            return (
                WorkloadType::TensorCompute,
                HardwareTarget {
                    node_friendly_name: self.hardware_inventory["AMD_INSTINCT"][0].clone(),
                    primary_reason: "Routed to MI50 Matrix Engines / BC250 Grid for tensor execution parallelism across pipeline-parallel layers.".into(),
                },
            );
        }

        // ── Ray-Tracing Patterns ──
        if lower.contains("unreal")
            || lower.contains("omniverse")
            || lower.contains("bakeraytracing")
            || lower.contains("bvh")
            || lower.contains("optix")
            || lower.contains("pathtracing")
            || lower.contains("raytracing")
            || lower.contains("raytrace")
            || lower.contains("nvnavidiaraytracing")
        {
            return (
                WorkloadType::RayTracing,
                HardwareTarget {
                    node_friendly_name: self.hardware_inventory["RTX_LEAD"][0].clone(),
                    primary_reason: "Routed to RTX 5090/4090 due to hardware BVH ray-intersection accelerators (RT cores + OptiX).".into(),
                },
            );
        }

        // ── Raster Graphics Patterns ──
        if lower.contains("unity")
            || lower.contains("raster")
            || lower.contains("blender")
            || lower.contains("framebuffer")
            || lower.contains("compositor")
            || lower.contains("viewport")
        {
            return (
                WorkloadType::RasterGraphics,
                HardwareTarget {
                    node_friendly_name: self.hardware_inventory["APU_GRID"][0].clone(),
                    primary_reason: "Routed to BC250 APU grid (144 GB pooled VRAM) for distributed rasterization and frame composition.".into(),
                },
            );
        }

        // ── Default Fallback: CPU ──
        (
            WorkloadType::GeneralCPU,
            HardwareTarget {
                node_friendly_name: self.hardware_inventory["XEON_BRAIN"][0].clone(),
                primary_reason: "No GPU-accelerated pattern detected. Defaulting to Xeon 6240 (72 threads, 512 GB RAM) for compilation, I/O, and scripting.".into(),
            },
        )
    }

    /// Returns a reference to the hardware inventory.
    #[must_use]
    pub fn inventory(&self) -> &HashMap<String, Vec<String>> {
        &self.hardware_inventory
    }
}

impl Default for ShoggothParser {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pytorch_detection() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("import torch.nn as nn");
        assert_eq!(workload, WorkloadType::TensorCompute);
    }

    #[test]
    fn test_alphafold_detection() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("from alphafold import model");
        assert_eq!(workload, WorkloadType::TensorCompute);
    }

    #[test]
    fn test_cuda_detection() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("tensor.cuda()");
        assert_eq!(workload, WorkloadType::TensorCompute);
    }

    #[test]
    fn test_unreal_detection() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("// Unreal Engine 5 Nanite pipeline");
        assert_eq!(workload, WorkloadType::RayTracing);
    }

    #[test]
    fn test_omniverse_detection() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("Omniverse Kit extension");
        assert_eq!(workload, WorkloadType::RayTracing);
    }

    #[test]
    fn test_raster_fallback() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("using UnityEngine;");
        assert_eq!(workload, WorkloadType::RasterGraphics);
    }

    #[test]
    fn test_generic_code_defaults_to_cpu() {
        let parser = ShoggothParser::new();
        let (workload, target) =
            parser.analyze_source_code("console.log('hello');");
        assert_eq!(workload, WorkloadType::GeneralCPU);
        assert!(target.node_friendly_name.contains("Xeon"));
    }

    #[test]
    fn test_case_insensitivity() {
        let parser = ShoggothParser::new();
        let (workload, _) = parser.analyze_source_code("TORCH.NN.Linear(20, 20)");
        assert_eq!(workload, WorkloadType::TensorCompute);
    }
}
