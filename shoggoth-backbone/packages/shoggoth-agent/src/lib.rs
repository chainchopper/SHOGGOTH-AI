// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-agent/src/lib.rs — Agentic orchestration engine.
//
// When a developer drops an existing project (PyTorch, Blender, Unity,
// Unreal Engine, AlphaFold, etc.) into the Shoggoth SDK, the agentic parser
// analyzes the source code AST to detect resource-heavy patterns and maps
// them to the optimal hardware in the Shoggoth fabric.
//
// This crate provides:
//   • parser.rs — AST keyword / pattern matching for workload classification.
//   • templates.rs — SDK onboarding manifest templates (shoggoth.toml).
//   • router.rs — Hardware-aware workload-to-node mapping engine.

pub mod parser;
pub mod templates;

use shoggoth_sdk::topology::{PhysicalResourceNode, ShoggothFabricPool, SpecializedCapability};

/// Result of the agentic analysis: what kind of workload and where to run it.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Classified workload type.
    pub workload: parser::WorkloadType,
    /// Suggested hardware target with justification.
    pub target: HardwareTarget,
    /// Which SDK template to offer the developer.
    pub suggested_template: templates::TemplateType,
    /// Confidence score for this routing decision (0.0–1.0).
    pub confidence: f32,
}

/// A hardware target selected by the agentic router.
#[derive(Debug, Clone)]
pub struct HardwareTarget {
    /// Friendly node name (e.g., "RTX 5090 (Edge)").
    pub node_friendly_name: String,
    /// Human-readable justification for the routing decision.
    pub primary_reason: String,
}

/// Orchestrates the full agentic pipeline: parse → classify → route → template.
#[derive(Debug)]
pub struct ShoggothAgent {
    /// The code parser / workload classifier.
    parser: parser::ShoggothParser,
    /// Current hardware topology snapshot for routing decisions.
    fabric_pool: ShoggothFabricPool,
}

impl ShoggothAgent {
    /// Creates a new agentic orchestrator with the default parser and lab topology.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: parser::ShoggothParser::new(),
            fabric_pool: shoggoth_sdk::topology::build_lab_topology(),
        }
    }

    /// Creates an agent with a custom fabric pool (e.g., after node discovery).
    #[must_use]
    pub fn with_fabric(fabric_pool: ShoggothFabricPool) -> Self {
        Self {
            parser: parser::ShoggothParser::new(),
            fabric_pool,
        }
    }

    /// Analyzes a source code snippet and returns a full routing decision.
    ///
    /// The pipeline:
    ///   1. Parse the code for workload signatures.
    ///   2. Classify the workload type.
    ///   3. Query the fabric pool for the best available hardware.
    ///   4. Select the appropriate SDK onboarding template.
    #[must_use]
    pub fn analyze_and_route(&self, source_code: &str) -> RoutingDecision {
        // Step 1+2: Parse and classify.
        let (workload, initial_target) = self.parser.analyze_source_code(source_code);

        // Step 3: Refine by querying actual live hardware.
        let capability = match workload {
            parser::WorkloadType::TensorCompute => SpecializedCapability::MatrixTensorCore,
            parser::WorkloadType::RayTracing => SpecializedCapability::HardwareRayTracing,
            parser::WorkloadType::RasterGraphics => SpecializedCapability::MassiveUnifiedAPU,
            parser::WorkloadType::GeneralCPU => SpecializedCapability::SystemCentralBrain,
        };

        let available_nodes = self.fabric_pool.request_pooled_resources(capability);
        let target = if let Some(best_node) = available_nodes.first() {
            HardwareTarget {
                node_friendly_name: best_node.node_id.clone(),
                primary_reason: format!(
                    "Routed to {} ({:?}, {} GB VRAM, {:.1} ms ping)",
                    best_node.node_id,
                    best_node.tier,
                    best_node.available_vram_gb,
                    best_node.network_ping_ms,
                ),
            }
        } else {
            // Fallback to the original parser suggestion.
            initial_target
        };

        // Step 4: Select the onboarding template.
        let suggested_template = templates::select_template(&workload);

        // Confidence: 0.9 if we found live hardware, 0.5 if we fell back.
        let confidence = if available_nodes.is_empty() {
            0.5
        } else {
            0.9
        };

        RoutingDecision {
            workload,
            target,
            suggested_template,
            confidence,
        }
    }

    /// Updates the fabric pool with live hardware data (e.g., after node discovery).
    pub fn update_fabric(&mut self, pool: ShoggothFabricPool) {
        self.fabric_pool = pool;
    }

    /// Returns a snapshot of the current hardware pool.
    #[must_use]
    pub fn fabric_snapshot(&self) -> &ShoggothFabricPool {
        &self.fabric_pool
    }
}

impl Default for ShoggothAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_pytorch_code() {
        let agent = ShoggothAgent::new();
        let code = "import torch.nn as nn; model = nn.Linear(20, 20).cuda()";
        let decision = agent.analyze_and_route(code);

        assert_eq!(decision.workload, parser::WorkloadType::TensorCompute);
        assert!(decision.confidence > 0.8);
        assert_eq!(
            decision.suggested_template,
            templates::TemplateType::HeavyCompute
        );
    }

    #[test]
    fn test_analyze_unreal_code() {
        let agent = ShoggothAgent::new();
        let code = "// Unreal Engine shader: BakeRayTracing for global illumination";
        let decision = agent.analyze_and_route(code);

        assert_eq!(decision.workload, parser::WorkloadType::RayTracing);
        assert_eq!(
            decision.suggested_template,
            templates::TemplateType::RenderFarm
        );
    }

    #[test]
    fn test_analyze_generic_code() {
        let agent = ShoggothAgent::new();
        let code = "console.log('hello world');";
        let decision = agent.analyze_and_route(code);

        assert_eq!(decision.workload, parser::WorkloadType::GeneralCPU);
    }
}
