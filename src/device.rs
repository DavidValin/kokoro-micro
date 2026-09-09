//! Where the model runs: the CPU, or a GPU execution provider of ONNX Runtime.
//!
//! GPU support is optional and compiled in with the `cuda` cargo feature.
//! Without it every device choice resolves to the CPU, which is also what a
//! [`Device::Auto`] build does on a machine with no usable GPU - registering
//! an execution provider is a load-time decision, and a failed registration
//! is not an error unless the caller asked for that device by name.

use std::path::Path;

use ort::session::{
    builder::{GraphOptimizationLevel, SessionBuilder},
    Session,
};

/// Where the ONNX model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    /// Use a GPU when the crate was built with a GPU feature and one can be
    /// used, otherwise the CPU. Loading never fails because of the GPU: a
    /// provider that will not register, or will not take the model, is
    /// skipped. It says nothing about later inference - a card that is merely
    /// full loads fine and fails on the first run, which is what
    /// [`crate::TtsEngine::fallback_to_cpu`] is for.
    #[default]
    Auto,
    /// CPU only.
    Cpu,
    /// Require a GPU (first device): fails when no GPU provider is compiled
    /// in or it cannot be initialized.
    Gpu,
    /// Require a specific GPU by index.
    GpuIndex(i32),
}

impl Device {
    /// Parse `auto`, `cpu`, `gpu` or `gpu:<index>` (`cuda` and `cuda:<index>`
    /// are accepted as aliases), for crates that take a device on a command
    /// line or out of a config file.
    pub fn parse(s: &str) -> Result<Device, String> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "auto" => Ok(Device::Auto),
            "cpu" => Ok(Device::Cpu),
            "gpu" | "cuda" => Ok(Device::Gpu),
            other => {
                if let Some(idx) = other
                    .strip_prefix("gpu:")
                    .or_else(|| other.strip_prefix("cuda:"))
                {
                    let n: i32 = idx
                        .parse()
                        .map_err(|_| format!("invalid device index in '{}'", s))?;
                    Ok(Device::GpuIndex(n))
                } else {
                    Err(format!(
                        "unknown device '{}': use auto, cpu, gpu or gpu:<index>",
                        s
                    ))
                }
            }
        }
    }

    /// Whether this choice may end up on a GPU at all (everything but
    /// [`Device::Cpu`]).
    pub fn wants_gpu(self) -> bool {
        !matches!(self, Device::Cpu)
    }

    fn requires_gpu(self) -> bool {
        matches!(self, Device::Gpu | Device::GpuIndex(_))
    }

    fn index(self) -> i32 {
        match self {
            Device::GpuIndex(i) => i,
            _ => 0,
        }
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Device::Auto => f.write_str("auto"),
            Device::Cpu => f.write_str("cpu"),
            Device::Gpu => f.write_str("gpu"),
            Device::GpuIndex(i) => write!(f, "gpu:{}", i),
        }
    }
}

/// Name of the CPU execution provider as reported by [`Backend`].
pub const CPU_PROVIDER: &str = "CPU";

/// The execution provider a session actually runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    /// `"CPU"` or the ONNX Runtime provider name, e.g.
    /// `"CUDAExecutionProvider"`.
    pub provider: String,
    /// Device index for GPU providers (0 on the CPU).
    pub device_index: i32,
}

impl Backend {
    pub fn cpu() -> Backend {
        Backend {
            provider: CPU_PROVIDER.to_string(),
            device_index: 0,
        }
    }

    pub fn is_gpu(&self) -> bool {
        self.provider != CPU_PROVIDER
    }
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_gpu() {
            write!(f, "{} (device {})", self.provider, self.device_index)
        } else {
            f.write_str(CPU_PROVIDER)
        }
    }
}

/// Names of the GPU execution providers compiled into this build, in the
/// order [`Device::Auto`] tries them. Empty without any GPU cargo feature.
pub fn compiled_gpu_providers() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut v: Vec<&'static str> = Vec::new();
    #[cfg(feature = "cuda")]
    v.push("CUDAExecutionProvider");
    v
}

/// Whether this build can use a GPU at all.
pub fn gpu_support_compiled() -> bool {
    !compiled_gpu_providers().is_empty()
}

/// Register `ep` on a fresh session builder. The dispatch is marked
/// `error_on_failure` on purpose: ONNX Runtime otherwise drops silently to
/// the CPU, and then [`Backend`] would report a GPU the model is not on.
#[allow(dead_code)]
fn with_ep(
    provider: &str,
    ep: ort::ep::ExecutionProviderDispatch,
) -> Result<SessionBuilder, String> {
    Session::builder()
        .map_err(|e| format!("failed to create session builder: {}", e))?
        .with_execution_providers([ep.error_on_failure()])
        .map_err(|e| format!("register {}: {}", provider, e))
}

/// Create a session builder registered with the given provider (by the names
/// of [`compiled_gpu_providers`]). Errors when the provider cannot be
/// initialized (missing driver / runtime libraries, no device, ...).
#[allow(unused_variables)]
fn builder_for(provider: &str, device_index: i32) -> Result<SessionBuilder, String> {
    match provider {
        #[cfg(feature = "cuda")]
        "CUDAExecutionProvider" => with_ep(
            provider,
            ort::ep::CUDA::default()
                .with_device_id(device_index)
                .build(),
        ),
        other => Err(format!("execution provider {} is not compiled in", other)),
    }
}

/// Reject a device this build can never provide, without touching ONNX
/// Runtime. Callers use it to fail before doing expensive work (downloading a
/// model, say) on a request that cannot be satisfied whatever the machine has.
pub fn check_device(device: Device) -> Result<(), String> {
    if device.requires_gpu() && !gpu_support_compiled() {
        return Err(format!(
            "device {} requested but this build has no GPU support: rebuild with \
             `--features cuda`",
            device
        ));
    }
    Ok(())
}

/// Resolve `device` to the backend a load would use, without loading anything.
/// [`Device::Gpu`] / [`Device::GpuIndex`] return an error instead of falling
/// back. This registers the execution provider to find out, so it is a probe,
/// not part of loading a model - [`load_session`] does its own, once.
pub fn resolve_backend(device: Device) -> Result<Backend, String> {
    if !device.wants_gpu() {
        return Ok(Backend::cpu());
    }
    let providers = compiled_gpu_providers();
    if providers.is_empty() {
        if device.requires_gpu() {
            return Err(format!(
                "device {} requested but this build has no GPU support: rebuild with \
                 `--features cuda`",
                device
            ));
        }
        return Ok(Backend::cpu());
    }
    let mut errors = Vec::new();
    for p in providers {
        match builder_for(p, device.index()) {
            Ok(_) => {
                return Ok(Backend {
                    provider: p.to_string(),
                    device_index: device.index(),
                })
            }
            Err(e) => errors.push(format!("{}: {}", p, e)),
        }
    }
    if device.requires_gpu() {
        Err(format!(
            "device {} requested but no GPU execution provider could be initialized:\n  {}",
            device,
            errors.join("\n  ")
        ))
    } else {
        Ok(Backend::cpu())
    }
}

/// A session builder with this crate's optimization level already set.
fn tuned(builder: SessionBuilder) -> Result<SessionBuilder, String> {
    builder
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("failed to set optimization level: {}", e))
}

fn cpu_builder() -> Result<SessionBuilder, String> {
    tuned(Session::builder().map_err(|e| format!("failed to create session builder: {}", e))?)
}

/// Load an ONNX model for `device`, and report where it ended up.
///
/// Each execution provider is initialized once here, on the builder the model
/// is then committed to - resolving the device first and building a second
/// time would pay for provider start-up twice, and on a contended GPU the two
/// answers need not even agree. For [`Device::Auto`] a provider that fails to
/// register *or* to take the model is passed over; the CPU always ends the
/// list. [`Device::Gpu`] and [`Device::GpuIndex`] report those failures
/// instead of quietly running somewhere else.
pub fn load_session<P: AsRef<Path>>(path: P, device: Device) -> Result<(Session, Backend), String> {
    check_device(device)?;
    let path = path.as_ref();
    let model_bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read model file {}: {}", path.display(), e))?;

    let mut errors = Vec::new();
    if device.wants_gpu() {
        for provider in compiled_gpu_providers() {
            let backend = Backend {
                provider: provider.to_string(),
                device_index: device.index(),
            };
            let attempt = builder_for(provider, device.index())
                .and_then(tuned)
                .and_then(|mut b| {
                    b.commit_from_memory(&model_bytes)
                        .map_err(|e| format!("failed to load the model: {}", e))
                });
            match attempt {
                Ok(session) => return Ok((session, backend)),
                Err(e) => errors.push(format!("{}: {}", provider, e)),
            }
        }
        if device.requires_gpu() {
            return Err(format!(
                "device {} requested but no GPU execution provider could load {}:\n  {}",
                device,
                path.display(),
                errors.join("\n  ")
            ));
        }
    }

    let mut cpu = cpu_builder()?;
    let session = cpu
        .commit_from_memory(&model_bytes)
        .map_err(|e| format!("Failed to load model {} on CPU: {}", path.display(), e))?;
    Ok((session, Backend::cpu()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_parsing() {
        assert_eq!(Device::parse("auto").unwrap(), Device::Auto);
        assert_eq!(Device::parse("CPU").unwrap(), Device::Cpu);
        assert_eq!(Device::parse("gpu").unwrap(), Device::Gpu);
        assert_eq!(Device::parse("cuda:1").unwrap(), Device::GpuIndex(1));
        assert!(Device::parse("tpu").is_err());
        assert!(Device::parse("gpu:x").is_err());
        assert_eq!(Device::default(), Device::Auto);
    }

    #[test]
    fn cpu_and_auto_resolve_without_gpu_features() {
        assert_eq!(resolve_backend(Device::Cpu).unwrap(), Backend::cpu());
        let auto = resolve_backend(Device::Auto).unwrap();
        if !gpu_support_compiled() {
            assert_eq!(auto, Backend::cpu());
            assert!(resolve_backend(Device::Gpu).is_err());
        }
        assert_eq!(Backend::cpu().to_string(), "CPU");
    }
}
