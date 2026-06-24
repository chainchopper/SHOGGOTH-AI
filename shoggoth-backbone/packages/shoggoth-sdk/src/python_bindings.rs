// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/python_bindings.rs — PyO3 Python bindings.
//
// Exposes the Shoggoth SDK to Python via the `shoggoth` module.
// Enabled via the `python-bindings` feature flag.
//
// Usage from Python:
//   import shoggoth
//
//   pool = shoggoth.FabricPool()
//   topology = pool.get_topology()
//
//   engine = shoggoth.RuntimeEngine()
//   state = await engine.execute_frame()
//
//   chain = shoggoth.SyncChain(14)  # 14 GPU nodes
//   await chain.synchronize("node-01", tile_payload)

use pyo3::prelude::*;

// ── FabricPool ─────────────────────────────────────────────────────────────────

#[pyclass(name = "FabricPool")]
struct PyFabricPool {
    inner: crate::topology::ShoggothFabricPool,
}

#[pymethods]
impl PyFabricPool {
    #[new]
    fn new() -> Self {
        Self {
            inner: crate::topology::build_lab_topology(),
        }
    }

    fn get_topology(&self) -> PyResult<String> {
        let nodes: Vec<_> = self.inner.active_nodes.values().collect();
        serde_json::to_string(&nodes).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })
    }

    fn node_count(&self) -> usize {
        self.inner.active_nodes.len()
    }

    fn total_vram_gb(&self) -> f64 {
        self.inner.total_vram_gb()
    }

    fn full_shoggoth_count(&self) -> usize {
        self.inner.full_shoggoth_nodes().len()
    }

    fn __repr__(&self) -> String {
        format!(
            "FabricPool(nodes={}, vram={:.1}GB, full_shoggoths={})",
            self.inner.active_nodes.len(),
            self.inner.total_vram_gb(),
            self.inner.full_shoggoth_nodes().len(),
        )
    }
}

// ── RuntimeEngine ──────────────────────────────────────────────────────────────

#[pyclass(name = "RuntimeEngine")]
struct PyRuntimeEngine {
    inner: crate::runtime::ShoggothRuntimeEngine,
}

#[pymethods]
impl PyRuntimeEngine {
    #[new]
    fn new() -> Self {
        Self {
            inner: crate::runtime::ShoggothRuntimeEngine::new(),
        }
    }

    fn execute_frame<'a>(&'a mut self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let state = self.inner.execute_frame().await;
            Ok(Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("frame_index", state.frame_index).ok();
                dict.set_item(
                    "player_position",
                    (
                        state.player_position.0,
                        state.player_position.1,
                        state.player_position.2,
                    ),
                )
                .ok();
                dict.set_item("quality_tier", format!("{:?}", state.quality_tier))
                    .ok();
                dict.to_object(py)
            }))
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeEngine(frame={}, cloud_timeout_ms={})",
            self.inner.current_frame,
            self.inner.cloud_timeout_ms,
        )
    }
}

// ── SyncChain ──────────────────────────────────────────────────────────────────

#[pyclass(name = "SyncChain")]
struct PySyncChain {
    inner: std::sync::Arc<crate::sync_chain::ShoggothSyncChain>,
}

#[pymethods]
impl PySyncChain {
    #[new]
    fn new(node_count: usize) -> PyResult<Self> {
        if node_count == 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Node count must be at least 1",
            ));
        }
        Ok(Self {
            inner: std::sync::Arc::new(crate::sync_chain::ShoggothSyncChain::new(node_count)),
        })
    }

    fn __repr__(&self) -> String {
        format!("SyncChain(nodes={})", self.inner.total_nodes)
    }
}

// ── Helper Functions ───────────────────────────────────────────────────────────

/// Returns the Shoggoth SDK version.
#[pyfunction]
fn version() -> String {
    crate::VERSION.to_string()
}

/// Returns the protocol version for wire compatibility.
#[pyfunction]
fn protocol_version() -> u16 {
    crate::PROTOCOL_VERSION
}

// ── Module Definition ──────────────────────────────────────────────────────────

#[pymodule]
fn shoggoth(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFabricPool>()?;
    m.add_class::<PyRuntimeEngine>()?;
    m.add_class::<PySyncChain>()?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(protocol_version, m)?)?;
    Ok(())
}
