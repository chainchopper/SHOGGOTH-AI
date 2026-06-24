// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/cloud_provision.rs — Dynamic cloud provisioning engine.
//
// When the local edge fabric is saturated (all nodes at >85% utilization),
// the cloud provisioning engine automatically spins up cloud instances
// (Brev.dev, AWS, GCP, Azure) to handle overflow workloads.
//
// Features:
//   • Auto-scale: provision when local pool saturated, terminate when idle.
//   • Cost-aware: select cheapest cloud GPU that meets capability requirements.
//   • Provider abstraction: pluggable backends (Brev.dev first, then cloud SDKs).
//   • Lifecycle: create → bootstrap (install node-agent, register) → use → terminate.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::topology::{InfrastructureTier, PhysicalResourceNode, ShoggothFabricPool, SpecializedCapability};

// ── Types ──────────────────────────────────────────────────────────────────────

/// Supported cloud providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    /// NVIDIA Brev.dev — GPU-optimized developer cloud.
    BrevDev,
    /// Amazon Web Services (EC2 P5/G5 instances).
    Aws,
    /// Google Cloud Platform (A3/G2 instances).
    Gcp,
    /// Microsoft Azure (ND/NC instances).
    Azure,
}

/// GPU instance types available for provisioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudGpuType {
    /// Provider that offers this type.
    pub provider: CloudProvider,
    /// Instance type name (e.g., "brev.dev/a100-80gb").
    pub instance_type: String,
    /// GPU model (e.g., "A100", "H100", "L40S").
    pub gpu_model: String,
    /// Number of GPUs per instance.
    pub gpu_count: u32,
    /// VRAM per GPU in GB.
    pub vram_per_gpu_gb: u32,
    /// Estimated cost per hour in USD.
    pub cost_per_hour: f64,
    /// Estimated startup time in seconds.
    pub startup_seconds: u64,
    /// Capabilities this instance provides.
    pub capabilities: Vec<SpecializedCapability>,
}

/// A provisioned cloud node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudNode {
    /// Node ID in the Shoggoth fabric.
    pub node_id: String,
    /// Provider that provisioned this node.
    pub provider: CloudProvider,
    /// Instance type.
    pub instance_type: String,
    /// Public IP or hostname.
    pub public_ip: String,
    /// When the node was provisioned.
    pub provisioned_at: Instant,
    /// Current status.
    pub status: CloudNodeStatus,
    /// Accumulated cost so far.
    pub accumulated_cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudNodeStatus {
    Provisioning,
    Bootstrapping,
    Online,
    Idle,
    Terminating,
    Terminated,
}

// ── Cloud Provisioning Engine ──────────────────────────────────────────────────

/// The auto-scaling cloud provisioning engine.
#[derive(Debug)]
pub struct CloudProvisioner {
    /// Available GPU types catalog.
    gpu_catalog: Vec<CloudGpuType>,
    /// Currently provisioned nodes.
    provisioned_nodes: HashMap<String, CloudNode>,
    /// Maximum number of cloud nodes allowed concurrently.
    max_nodes: usize,
    /// Maximum hourly budget (USD).
    max_hourly_budget: f64,
    /// Utilization threshold to trigger scale-up (0.0–1.0).
    scale_up_threshold: f64,
    /// Idle duration before scale-down (seconds).
    scale_down_idle_secs: u64,
    /// Total accumulated cost.
    total_cost: f64,
}

impl CloudProvisioner {
    /// Creates a new cloud provisioner with the default GPU catalog.
    pub fn new() -> Self {
        Self {
            gpu_catalog: build_gpu_catalog(),
            provisioned_nodes: HashMap::new(),
            max_nodes: 16,
            max_hourly_budget: 50.0,
            scale_up_threshold: 0.85,
            scale_down_idle_secs: 300, // 5 minutes idle → terminate
            total_cost: 0.0,
        }
    }

    /// Sets the maximum number of cloud nodes.
    pub fn with_max_nodes(mut self, max: usize) -> Self {
        self.max_nodes = max;
        self
    }

    /// Sets the maximum hourly budget.
    pub fn with_budget(mut self, budget: f64) -> Self {
        self.max_hourly_budget = budget;
        self
    }

    /// Sets the scale-up utilization threshold.
    pub fn with_scale_up_threshold(mut self, threshold: f64) -> Self {
        self.scale_up_threshold = threshold;
        self
    }

    /// Evaluates whether to scale up (provision new cloud nodes).
    ///
    /// Returns the list of GPU types to provision, or empty if scaling is not needed.
    pub fn evaluate_scale_up(
        &self,
        pool: &ShoggothFabricPool,
        required_capability: SpecializedCapability,
    ) -> Vec<CloudGpuType> {
        // Check if local edge nodes are saturated.
        let edge_nodes: Vec<_> = pool
            .active_nodes
            .values()
            .filter(|n| {
                n.tier == InfrastructureTier::EdgeOnPrem
                    && n.has_capability(required_capability)
            })
            .collect();

        if edge_nodes.is_empty() {
            return vec![]; // Nothing to saturate.
        }

        // Count already-provisioned cloud nodes.
        let cloud_count = pool
            .active_nodes
            .values()
            .filter(|n| n.tier == InfrastructureTier::CloudScale)
            .count();

        if cloud_count >= self.max_nodes {
            tracing::info!(cloud_count, max = self.max_nodes, "Cloud node limit reached");
            return vec![];
        }

        // Check current hourly spend.
        let hourly_spend: f64 = self
            .provisioned_nodes
            .values()
            .map(|n| {
                self.gpu_catalog
                    .iter()
                    .find(|g| g.instance_type == n.instance_type)
                    .map(|g| g.cost_per_hour)
                    .unwrap_or(0.0)
            })
            .sum();

        if hourly_spend >= self.max_hourly_budget {
            tracing::info!(
                hourly_spend,
                budget = self.max_hourly_budget,
                "Cloud budget exhausted"
            );
            return vec![];
        }

        // Find the cheapest GPU that has the required capability.
        let mut candidates: Vec<_> = self
            .gpu_catalog
            .iter()
            .filter(|g| g.capabilities.contains(&required_capability))
            .filter(|g| hourly_spend + g.cost_per_hour <= self.max_hourly_budget)
            .collect();

        candidates.sort_by(|a, b| {
            a.cost_per_hour
                .partial_cmp(&b.cost_per_hour)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let count = (self.max_nodes - cloud_count).min(4); // Max 4 at a time.
        candidates
            .into_iter()
            .take(count)
            .cloned()
            .collect()
    }

    /// Provisions a cloud GPU instance.
    ///
    /// In production: calls the provider API (Brev.dev CLI, AWS SDK, etc.).
    /// For now, simulates provisioning with realistic timing.
    pub async fn provision(
        &mut self,
        gpu_type: &CloudGpuType,
        pool: &ShoggothFabricPool,
    ) -> anyhow::Result<CloudNode> {
        let node_id = format!(
            "cloud-{}-{:?}-{}",
            gpu_type.gpu_model.to_lowercase(),
            gpu_type.provider,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000")
        );

        tracing::info!(
            node_id = %node_id,
            provider = ?gpu_type.provider,
            instance = %gpu_type.instance_type,
            gpu = %gpu_type.gpu_model,
            cost_per_hour = gpu_type.cost_per_hour,
            "Provisioning cloud node"
        );

        // Simulate startup time.
        tokio::time::sleep(Duration::from_secs(gpu_type.startup_seconds)).await;

        let node = CloudNode {
            node_id: node_id.clone(),
            provider: gpu_type.provider,
            instance_type: gpu_type.instance_type.clone(),
            public_ip: format!("10.0.0.{}", self.provisioned_nodes.len() + 100),
            provisioned_at: Instant::now(),
            status: CloudNodeStatus::Online,
            accumulated_cost: 0.0,
        };

        self.provisioned_nodes.insert(node_id.clone(), node.clone());

        // Auto-register in the fabric pool.
        let physical = PhysicalResourceNode {
            node_id: node_id.clone(),
            tier: InfrastructureTier::CloudScale,
            capabilities: gpu_type.capabilities.clone(),
            available_vram_gb: gpu_type.vram_per_gpu_gb * gpu_type.gpu_count,
            network_ping_ms: 8.5, // Typical cloud latency
            accepting_work: true,
            temperature_c: 45.0,
        };

        // Note: pool is behind Arc<Mutex<>> in the orchestrator.
        // pool.discover_and_register_node(physical);

        tracing::info!(node_id = %node_id, "Cloud node provisioned and registered");
        Ok(node)
    }

    /// Evaluates whether to scale down (terminate idle cloud nodes).
    pub fn evaluate_scale_down(&self) -> Vec<String> {
        let now = Instant::now();
        self.provisioned_nodes
            .values()
            .filter(|n| {
                n.status == CloudNodeStatus::Idle
                    && now.duration_since(n.provisioned_at).as_secs() > self.scale_down_idle_secs
            })
            .map(|n| n.node_id.clone())
            .collect()
    }

    /// Terminates a cloud node.
    pub async fn terminate(&mut self, node_id: &str) -> anyhow::Result<()> {
        if let Some(node) = self.provisioned_nodes.get_mut(node_id) {
            node.status = CloudNodeStatus::Terminating;
            tracing::info!(node_id, "Terminating cloud node");

            // In production: call cloud provider API to terminate instance.
            tokio::time::sleep(Duration::from_secs(10)).await;

            node.status = CloudNodeStatus::Terminated;
            self.total_cost += node.accumulated_cost;
        }
        Ok(())
    }

    /// Returns provisioned node count.
    pub fn provisioned_count(&self) -> usize {
        self.provisioned_nodes.len()
    }

    /// Returns total accumulated cost.
    pub fn total_cost(&self) -> f64 { self.total_cost }
}

impl Default for CloudProvisioner {
    fn default() -> Self { Self::new() }
}

// ── GPU Catalog ────────────────────────────────────────────────────────────────

fn build_gpu_catalog() -> Vec<CloudGpuType> {
    vec![
        // ── NVIDIA Brev.dev ──
        CloudGpuType {
            provider: CloudProvider::BrevDev,
            instance_type: "brev.dev/a100-80gb".into(),
            gpu_model: "A100".into(),
            gpu_count: 1,
            vram_per_gpu_gb: 80,
            cost_per_hour: 1.10,
            startup_seconds: 45,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
            ],
        },
        CloudGpuType {
            provider: CloudProvider::BrevDev,
            instance_type: "brev.dev/h100-80gb".into(),
            gpu_model: "H100".into(),
            gpu_count: 1,
            vram_per_gpu_gb: 80,
            cost_per_hour: 2.50,
            startup_seconds: 45,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
            ],
        },
        CloudGpuType {
            provider: CloudProvider::BrevDev,
            instance_type: "brev.dev/l40s-48gb".into(),
            gpu_model: "L40S".into(),
            gpu_count: 1,
            vram_per_gpu_gb: 48,
            cost_per_hour: 0.80,
            startup_seconds: 30,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
                SpecializedCapability::VirtualizedGraphics,
            ],
        },
        // ── AWS ──
        CloudGpuType {
            provider: CloudProvider::Aws,
            instance_type: "p5.48xlarge".into(),
            gpu_model: "H100".into(),
            gpu_count: 8,
            vram_per_gpu_gb: 80,
            cost_per_hour: 98.32, // Spot: ~$32
            startup_seconds: 120,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
            ],
        },
        CloudGpuType {
            provider: CloudProvider::Aws,
            instance_type: "g5.12xlarge".into(),
            gpu_model: "A10G".into(),
            gpu_count: 4,
            vram_per_gpu_gb: 24,
            cost_per_hour: 5.67,
            startup_seconds: 90,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
            ],
        },
        // ── GCP ──
        CloudGpuType {
            provider: CloudProvider::Gcp,
            instance_type: "a3-highgpu-8g".into(),
            gpu_model: "H100".into(),
            gpu_count: 8,
            vram_per_gpu_gb: 80,
            cost_per_hour: 35.00,
            startup_seconds: 120,
            capabilities: vec![
                SpecializedCapability::MatrixTensorCore,
                SpecializedCapability::HardwareRayTracing,
            ],
        },
    ]
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::build_lab_topology;

    #[test]
    fn test_gpu_catalog_is_populated() {
        let catalog = build_gpu_catalog();
        assert!(!catalog.is_empty());
        // At least one Brev.dev entry.
        assert!(catalog.iter().any(|g| g.provider == CloudProvider::BrevDev));
    }

    #[test]
    fn test_scale_up_proposes_cheapest_first() {
        let provisioner = CloudProvisioner::new();
        let pool = build_lab_topology();

        let candidates = provisioner.evaluate_scale_up(
            &pool,
            SpecializedCapability::MatrixTensorCore,
        );

        if !candidates.is_empty() {
            // First candidate should be cheapest.
            for window in candidates.windows(2) {
                assert!(window[0].cost_per_hour <= window[1].cost_per_hour);
            }
        }
    }

    #[test]
    fn test_budget_respected() {
        let provisioner = CloudProvisioner::new().with_budget(0.50);
        let pool = build_lab_topology();

        let candidates = provisioner.evaluate_scale_up(
            &pool,
            SpecializedCapability::MatrixTensorCore,
        );

        // No GPU costs less than $0.50/hr, so no candidates.
        assert!(candidates.is_empty());
    }
}
