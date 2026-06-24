# Migrating to Shoggoth — From Existing GPU Clusters

This guide helps teams currently using dedicated GPU setups transition to the
Shoggoth Mesh Machine with minimal friction.

## Migration Paths

### From: Kubernetes + GPU Operator

**Current setup**: K8s with NVIDIA GPU Operator, one GPU per pod.

```yaml
# Old: one pod = one GPU
apiVersion: v1
kind: Pod
spec:
  containers:
    - resources:
        limits:
          nvidia.com/gpu: 1
```

**Shoggoth equivalent**:
```yaml
# New: DaemonSet runs node-agent on every GPU node.
# Pods request compute via the Shoggoth SDK instead of K8s GPU resources.
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: shoggoth-node-agent
spec:
  template:
    spec:
      containers:
        - image: ghcr.io/chainchopper/shoggoth-node-agent:latest
```

**Impact**: Zero code changes to existing pods. Shoggoth intercepts Vulkan/DX12 calls
via the layer interceptor, distributing work across all GPUs automatically.

---

### From: Slurm HPC Cluster

**Current setup**: `sbatch` with GPU allocation.

```bash
# Old
sbatch --gres=gpu:rtx5090:1 train.sh
```

**Shoggoth equivalent**:
```bash
# New: launch via Shoggoth CLI, no GPU count needed.
shoggoth launch pytorch-hybrid
```

**Impact**: The agentic parser detects PyTorch training and auto-routes to the optimal
GPU mix. No manual `--gres` required.

---

### From: Dedicated Workstation (Single Developer)

**Current setup**: One RTX 4090, local PyTorch/Jupyter, occasional OOM.

```python
# Old
model = torch.nn.Linear(20, 20).cuda()
# RuntimeError: CUDA out of memory.
```

**Shoggoth equivalent**:
```python
# New: identical code, but Shoggoth shards layers across available GPUs.
from shoggoth_client import ShoggothClient

async with ShoggothClient() as client:
    # Shoggoth auto-routes this to MI50 + BC250 + RTX pipeline.
    result = await client.analyze("import torch.nn as nn; model = nn.Linear(20, 20).cuda()")
    print(f"Will use: {result.target_node}")
```

**Impact**: Same Python code. No changes to model architecture. The orchestrator detects
the workload and shards layers across the entire fabric.

---

### From: Render Farm (Blender / Unreal)

**Current setup**: Multiple machines, each rendering a different frame.

```bash
# Old: launch a render on each machine.
blender -b scene.blend -f 1   # Machine A
blender -b scene.blend -f 2   # Machine B
```

**Shoggoth equivalent**:
```bash
# New: single command, frames distributed automatically.
shoggoth launch render-farm
```

**Impact**: The orchestrator splits each frame into tiles, distributes tiles across
all GPUs (RT for center, BC250 for periphery), and composites into a single output.

---

### From: Code Without Any GPU Awareness

**Current setup**: Pure CPU scripts, no GPU utilization.

```python
# Old
result = heavy_computation(data)  # Runs on CPU only.
```

**Shoggoth equivalent**:
```python
# New: same code, but use the Shoggoth SDK context manager.
from shoggoth_client import ShoggothClient

async def accelerated_compute():
    async with ShoggothClient() as client:
        # Agentic parser detects heavy loops and suggests GPU acceleration.
        result = await client.analyze(open("script.py").read())
        if result.confidence > 0.5:
            await client.launch("generic", "my-script")
```

**Impact**: Shoggoth analyzes the source, detects compute patterns, and recommends
(or auto-launches) the appropriate GPU-accelerated template.

---

## Migration Checklist

- [ ] Install Shoggoth node-agent on all GPU machines.
  ```bash
  curl -fsSL https://get.shoggoth.dev | bash
  # Or Docker:
  docker run --network host --privileged --gpus all \
    ghcr.io/chainchopper/shoggoth-node-agent:latest
  ```
- [ ] Deploy orchestrator (one per cluster).
  ```bash
  docker run --network host \
    ghcr.io/chainchopper/shoggoth-orchestrator:latest
  ```
- [ ] Verify fabric topology.
  ```bash
  shoggoth topology
  # Should list all 19 nodes.
  ```
- [ ] Run GEMM benchmark to validate cross-vendor throughput.
  ```bash
  shoggoth benchmark gemm
  ```
- [ ] Install SDK in your project.
  - Python: `pip install shoggoth-client`
  - Rust: `cargo add shoggoth-sdk`
  - C++: Link `libshoggoth_sdk.so` via CMake.
- [ ] Run your existing workload — it auto-routes.
- [ ] Optional: create a `shoggoth.toml` for explicit hardware preferences.

## Breaking Changes (None)

Shoggoth is designed as a **drop-in accelerator**. Existing code paths continue to
work unchanged. The only addition is optional SDK calls for explicit control.

## Performance Expectations

| Workload | Old (Single GPU) | Shoggoth (Full Fabric) | Speedup |
|----------|-----------------|----------------------|---------|
| ResNet-50 training | 15 min (RTX 4090) | ~3 min | ~5× |
| 4K Blender frame | 45 sec (RTX 5090) | ~8 sec (tiled across 14 GPUs) | ~5.6× |
| AlphaFold inference | 12 min (RTX 3090) | ~2 min (MI50 + BC250) | ~6× |
| FASTA parsing (100 GB) | 8 min (CPU) | ~1 min (Xeon + BC250) | ~8× |

*Note: Benchmarks simulated. Actual results depend on network topology, model structure, and workload parallelism.*
