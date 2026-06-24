// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-agent/src/templates.rs — SDK onboarding manifests.
//
// Each template provides a shoggoth.toml boilerplate that developers can
// drop into their existing projects (Unreal, PyTorch, Blender, etc.) to
// onboard to the Shoggoth fabric without code changes.

use crate::parser::WorkloadType;

// ── Types ──────────────────────────────────────────────────────────────────────

/// Pre-configured workflow templates for the Shoggoth Launchpad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateType {
    /// Blender / Unreal / Omniverse: BVH shard to RT cores, BC250 grid as distributed rasterizers.
    RenderFarm,
    /// PyTorch / AlphaFold / CUDA: pipeline-parallel layer sharding across MI50 + BC250 + RTX.
    HeavyCompute,
    /// Unity / Unreal game: split UI/Sim (edge) + lighting/AI (cloud).
    AsyncGameRuntime,
    /// FASTA / AlphaGenome: ScyllaDB shard-per-core + BC250 alignment vectorization.
    GenomicProcessing,
    /// Generic acceleration: auto-detect and route.
    Generic,
}

impl TemplateType {
    /// Human-readable template name for the Launchpad UI.
    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::RenderFarm => "Render Farm (Blender / Unreal / Omniverse)",
            Self::HeavyCompute => "Heavy Compute (PyTorch / AlphaFold / CUDA)",
            Self::AsyncGameRuntime => "Async Game Runtime (Edge + Cloud Split)",
            Self::GenomicProcessing => "Genomic Processing (FASTA / AlphaGenome)",
            Self::Generic => "Generic Acceleration (Auto-Detect)",
        }
    }

    /// Generates the shoggoth.toml manifest for this template.
    #[must_use]
    pub fn generate_manifest(&self, project_name: &str) -> String {
        let header = format!(
            r#"# Shoggoth SDK Manifest
# Generated for: {project_name}
# Template: {}
# Date: automatically generated
"#,
            self.display_name()
        );

        match self {
            Self::RenderFarm => format!(
                "{header}\n\
                [workload]\n\
                type = \"render-farm\"\n\
                engine = \"auto-detect\"  # blender | unreal | omniverse\n\
                \n\
                [hardware.routing]\n\
                ray_tracing = [\"nvidia-rtx-5090\", \"nvidia-rtx-4090\", \"nvidia-rtx-3090\"]\n\
                rasterization = [\"bc250-apu-grid\"]\n\
                video_encode = [\"amd-v620\"]\n\
                \n\
                [streaming]\n\
                target_resolution = \"3840x2160\"  # 4K default\n\
                target_framerate = 60\n\
                adaptive_bitrate = true\n\
                encoder = \"nvenc\"  # nvenc | amf | software\n\
                \n\
                [network]\n\
                delta_compression = true\n\
                spatial_hashing = true\n\
                asset_precache = true\n"
            ),
            Self::HeavyCompute => format!(
                "{header}\n\
                [workload]\n\
                type = \"heavy-compute\"\n\
                framework = \"auto-detect\"  # pytorch | alphafold | cuda | triton\n\
                precision = \"bf16\"  # fp32 | fp16 | bf16 | int8\n\
                \n\
                [hardware.routing]\n\
                embedding_layers = [\"amd-mi50-cluster\"]\n\
                transformer_blocks = [\"bc250-apu-grid\"]\n\
                output_heads = [\"nvidia-rtx-5090\"]\n\
                parameter_server = \"xeon-brain-01\"\n\
                \n\
                [training]\n\
                lora_rank = 64\n\
                gradient_accumulation_steps = 8\n\
                pipeline_parallelism = true\n\
                tensor_parallelism = false\n\
                \n\
                [network]\n\
                activation_only = true  # Never transfer weights over network\n\
                model_precache = true\n"
            ),
            Self::AsyncGameRuntime => format!(
                "{header}\n\
                [workload]\n\
                type = \"async-game-runtime\"\n\
                engine = \"auto-detect\"  # unreal | unity | custom\n\
                target_resolution = \"7680x4320\"  # 8K default, scalable to 16K\n\
                \n\
                [edge]\n\
                player_input = \"xeon-brain-01\"\n\
                ui_render = \"nvidia-rtx-5090\"\n\
                network_prediction = \"xeon-brain-01\"\n\
                frame_delivery = \"nvidia-rtx-5090\"\n\
                \n\
                [cloud]\n\
                global_illumination = [\"nvidia-rtx-4090\", \"cloud-tensor-nodes\"]\n\
                secondary_bounces = [\"bc250-apu-grid\"]\n\
                ai_behaviors = [\"amd-mi50-cluster\"]\n\
                world_state = \"xeon-brain-01\"\n\
                \n\
                [sync]\n\
                frame_barrier = true\n\
                deterministic_tick = true\n\
                cloud_timeout_ms = 16\n"
            ),
            Self::GenomicProcessing => format!(
                "{header}\n\
                [workload]\n\
                type = \"genomic-processing\"\n\
                pipeline = \"auto-detect\"  # fasta | alignment | annotation\n\
                \n\
                [data]\n\
                reference_genome = \"/data/reference/hg38.fa\"\n\
                scylla_keyspace = \"genex\"\n\
                scylla_nodes = [\"localhost:9042\"]\n\
                \n\
                [hardware.routing]\n\
                fasta_parsing = \"xeon-brain-01\"\n\
                alignment = [\"bc250-apu-grid\"]\n\
                visualization = [\"nvidia-rtx-5090\", \"amd-v620\"]\n\
                scylla_loading = \"xeon-brain-01\"  # 72 shard connections\n\
                \n\
                [escrow]\n\
                validation_contracts = true\n\
                milestone_verification = \"bc250-apu-grid\"\n"
            ),
            Self::Generic => format!(
                "{header}\n\
                [workload]\n\
                type = \"auto-detect\"\n\
                \n\
                [hardware]\n\
                # Leave empty for automatic routing by the agentic parser.\n\
                # The orchestrator will detect your workload type and assign\n\
                # the optimal hardware automatically.\n\
                \n\
                [preferences]\n\
                max_network_bandwidth_mbps = 1000\n\
                prefer_edge_over_cloud = true\n\
                allow_cloud_fallback = true\n"
            ),
        }
    }
}

// ── Template Selection ─────────────────────────────────────────────────────────

/// Selects the best SDK template for a given workload type.
#[must_use]
pub fn select_template(workload: &WorkloadType) -> TemplateType {
    match workload {
        WorkloadType::RayTracing => TemplateType::RenderFarm,
        WorkloadType::TensorCompute => TemplateType::HeavyCompute,
        WorkloadType::RasterGraphics => TemplateType::AsyncGameRuntime,
        WorkloadType::GeneralCPU => TemplateType::Generic,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_farm_template() {
        let manifest = TemplateType::RenderFarm.generate_manifest("blender-test");
        assert!(manifest.contains("render-farm"));
        assert!(manifest.contains("nvidia-rtx-5090"));
        assert!(manifest.contains("bc250-apu-grid"));
        assert!(manifest.contains("blender-test"));
    }

    #[test]
    fn test_heavy_compute_template() {
        let manifest = TemplateType::HeavyCompute.generate_manifest("pytorch-test");
        assert!(manifest.contains("heavy-compute"));
        assert!(manifest.contains("bc250-apu-grid"));
        assert!(manifest.contains("activation_only"));
        assert!(manifest.contains("pytorch-test"));
    }

    #[test]
    fn test_game_runtime_template() {
        let manifest = TemplateType::AsyncGameRuntime.generate_manifest("unreal-game");
        assert!(manifest.contains("async-game-runtime"));
        assert!(manifest.contains("cloud_timeout_ms = 16"));
        assert!(manifest.contains("global_illumination"));
    }

    #[test]
    fn test_genomic_template() {
        let manifest = TemplateType::GenomicProcessing.generate_manifest("genex-test");
        assert!(manifest.contains("genomic-processing"));
        assert!(manifest.contains("scylla_keyspace"));
        assert!(manifest.contains("milestone_verification"));
    }

    #[test]
    fn test_template_selection_from_workload() {
        assert_eq!(
            select_template(&WorkloadType::RayTracing),
            TemplateType::RenderFarm
        );
        assert_eq!(
            select_template(&WorkloadType::TensorCompute),
            TemplateType::HeavyCompute
        );
        assert_eq!(
            select_template(&WorkloadType::RasterGraphics),
            TemplateType::AsyncGameRuntime
        );
        assert_eq!(
            select_template(&WorkloadType::GeneralCPU),
            TemplateType::Generic
        );
    }

    #[test]
    fn test_all_templates_have_display_names() {
        for template in &[
            TemplateType::RenderFarm,
            TemplateType::HeavyCompute,
            TemplateType::AsyncGameRuntime,
            TemplateType::GenomicProcessing,
            TemplateType::Generic,
        ] {
            assert!(!template.display_name().is_empty());
            let manifest = template.generate_manifest("test");
            assert!(!manifest.is_empty());
        }
    }
}
