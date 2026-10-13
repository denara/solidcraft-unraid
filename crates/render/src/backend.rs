//! Capability discovery and strict request resolution. This is metadata, not a render-job API.
//! GPU hosts may describe their own executors later; no device or UI types belong here.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Maximum ASCII bytes in a backend or executor identifier.
pub const MAX_RENDER_ID_BYTES: usize = 64;

macro_rules! render_id {
    ($name:ident) => {
        #[doc = "A validated, bounded identifier: lowercase ASCII letter followed by letters, digits, '.', '_' or '-'."]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: &str) -> Result<Self, ResolveError> {
                if value.is_empty()
                    || value.len() > MAX_RENDER_ID_BYTES
                    || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                    || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
                {
                    return Err(ResolveError::InvalidId);
                }
                Ok(Self(value.into()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ResolveError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(&value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

render_id!(BackendId);
render_id!(ExecutorId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    Quick,
    Preview,
    Final,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderOperation {
    Viewport,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderFormat {
    Png,
    Exr,
}

/// Features callers can require. Listing a variant does not advertise support for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderFeature {
    Hdri,
    Pbr,
    Refraction,
    Progressive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable { reason: String },
}

/// A supported combination, rather than the Cartesian product of independent feature flags.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProfile {
    pub mode: RenderMode,
    pub operation: RenderOperation,
    /// Image profiles list their formats; viewport profiles have none.
    pub formats: Vec<RenderFormat>,
    pub features: Vec<RenderFeature>,
}

/// Admission policy for the new resolver. Existing snapshot entry points retain their limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderLimits {
    pub max_dimension: u64,
    pub max_pixels: u64,
    pub max_scratch_bytes: u64,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self { max_dimension: 4096, max_pixels: 4_194_304, max_scratch_bytes: 256 * 1024 * 1024 }
    }
}

/// Estimated pixel-buffer cost, excluding scene/texture/geometry and codec-internal memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScratchLayout {
    pub sample_scale: u64,
    pub sample_bytes_per_pixel: u64,
    pub output_bytes_per_pixel: u64,
    pub encoding_bytes_per_pixel: u64,
}

impl ScratchLayout {
    /// Current rasterizer: 2× in each dimension, RGBA8 + f32 depth at both resolutions,
    /// plus an allowance for one RGBA copy and encoded output (8 bytes/output pixel).
    pub const CPU_RASTER: Self = Self { sample_scale: 2, sample_bytes_per_pixel: 8, output_bytes_per_pixel: 8, encoding_bytes_per_pixel: 8 };

    pub fn estimate(self, width: u64, height: u64) -> Result<u64, ResolveError> {
        let overflow = ResolveError::SizeOverflow;
        let pixels = width.checked_mul(height).ok_or(overflow.clone())?;
        let sw = width.checked_mul(self.sample_scale).ok_or(overflow.clone())?;
        let sh = height.checked_mul(self.sample_scale).ok_or(overflow.clone())?;
        let samples = sw.checked_mul(sh).ok_or(overflow.clone())?;
        let sampled = samples.checked_mul(self.sample_bytes_per_pixel).ok_or(overflow.clone())?;
        let output = pixels.checked_mul(self.output_bytes_per_pixel).ok_or(overflow.clone())?;
        let encoding = pixels.checked_mul(self.encoding_bytes_per_pixel).ok_or(overflow.clone())?;
        let bytes = sampled.checked_add(output).and_then(|n| n.checked_add(encoding)).ok_or(overflow.clone())?;
        // A request valid on a 64-bit host must not overflow allocations on wasm32.
        for value in [sw, sh, samples, pixels, bytes] {
            let value = usize::try_from(value).map_err(|_| overflow.clone())?;
            if value > isize::MAX as usize {
                return Err(overflow);
            }
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorDescriptor {
    pub id: ExecutorId,
    pub availability: Availability,
    pub profiles: Vec<RenderProfile>,
    pub limits: RenderLimits,
    pub scratch: ScratchLayout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDescriptor {
    pub id: BackendId,
    pub label: String,
    pub availability: Availability,
    pub executors: Vec<ExecutorDescriptor>,
}

/// Only strict resolution exists. Unknown JSON policies fail rather than imply a fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPolicy {
    #[default]
    Strict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    /// Omitted means the integrated backend, never "whichever engine happens to be available".
    #[serde(default)]
    pub backend: Option<BackendId>,
    pub mode: RenderMode,
    pub operation: RenderOperation,
    pub width: u64,
    pub height: u64,
    pub format: Option<RenderFormat>,
    #[serde(default)]
    pub features: Vec<RenderFeature>,
    #[serde(default)]
    pub policy: RenderPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedRequest {
    pub backend: BackendId,
    pub executor: ExecutorId,
    pub profile: RenderProfile,
    pub width: u64,
    pub height: u64,
    pub format: Option<RenderFormat>,
    pub estimated_scratch_bytes: u64,
    /// Always empty under the only supported policy, Strict.
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    InvalidId,
    InvalidRequest(&'static str),
    InvalidRegistry(&'static str),
    DuplicateBackend(BackendId),
    DuplicateExecutor { backend: BackendId, executor: ExecutorId },
    UnknownBackend(BackendId),
    BackendUnavailable(String),
    NoAvailableExecutor,
    UnsupportedProfile { mode: RenderMode, operation: RenderOperation },
    UnsupportedFormat,
    UnsupportedFeature(RenderFeature),
    InvalidDimensions,
    LimitExceeded { resource: &'static str, requested: u64, limit: u64 },
    SizeOverflow,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => write!(
                f,
                "render identifier must be 1–64 ASCII bytes, start with a lowercase letter and contain only lowercase letters, digits, '.', '_' or '-'"
            ),
            Self::InvalidRequest(reason) => write!(f, "invalid render request: {reason}"),
            Self::InvalidRegistry(reason) => write!(f, "invalid render registry: {reason}"),
            Self::DuplicateBackend(id) => write!(f, "duplicate render backend: {}", id.as_str()),
            Self::DuplicateExecutor { backend, executor } => write!(f, "duplicate executor {} in backend {}", executor.as_str(), backend.as_str()),
            Self::UnknownBackend(id) => write!(f, "unknown render backend: {}", id.as_str()),
            Self::BackendUnavailable(reason) => write!(f, "render backend unavailable: {reason}"),
            Self::NoAvailableExecutor => write!(f, "render backend has no available executor"),
            Self::UnsupportedProfile { mode, operation } => write!(f, "unsupported render profile: {mode:?}/{operation:?}"),
            Self::UnsupportedFormat => write!(f, "unsupported render output format for this profile"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported render feature: {feature:?}"),
            Self::InvalidDimensions => write!(f, "render dimensions must be positive"),
            Self::LimitExceeded { resource, requested, limit } => write!(f, "render {resource} exceeds limit: {requested} > {limit}"),
            Self::SizeOverflow => write!(f, "render buffer size overflows the supported address space"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// The existing CPU raster snapshot capability. No device is probed and no job is started.
pub fn builtin_backend() -> BackendDescriptor {
    BackendDescriptor {
        id: BackendId("builtin".into()),
        label: "Integrated".into(),
        availability: Availability::Available,
        executors: vec![ExecutorDescriptor {
            id: ExecutorId("cpu-raster".into()),
            availability: Availability::Available,
            profiles: vec![RenderProfile {
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                formats: vec![RenderFormat::Png],
                features: Vec::new(),
            }],
            limits: RenderLimits::default(),
            scratch: ScratchLayout::CPU_RASTER,
        }],
    }
}

fn validate_registry(backends: &[BackendDescriptor]) -> Result<(), ResolveError> {
    if backends.len() > 32 {
        return Err(ResolveError::InvalidRegistry("too many backends (maximum 32)"));
    }
    let mut ids = BTreeSet::new();
    for backend in backends {
        if !ids.insert(&backend.id) {
            return Err(ResolveError::DuplicateBackend(backend.id.clone()));
        }
        if backend.executors.len() > 16 {
            return Err(ResolveError::InvalidRegistry("too many executors (maximum 16 per backend)"));
        }
        let mut executors = BTreeSet::new();
        for executor in &backend.executors {
            if !executors.insert(&executor.id) {
                return Err(ResolveError::DuplicateExecutor { backend: backend.id.clone(), executor: executor.id.clone() });
            }
            if executor.profiles.len() > 16 || executor.scratch.sample_scale == 0 {
                return Err(ResolveError::InvalidRegistry("invalid profile count or zero sample scale"));
            }
            let mut profiles = BTreeSet::new();
            for profile in &executor.profiles {
                if profile.formats.len() > 16 || profile.features.len() > 32 {
                    return Err(ResolveError::InvalidRegistry("too many formats or features"));
                }
                if !profiles.insert((profile.mode, profile.operation)) {
                    return Err(ResolveError::InvalidRegistry("duplicate mode/operation profile"));
                }
                let formats: BTreeSet<_> = profile.formats.iter().collect();
                let features: BTreeSet<_> = profile.features.iter().collect();
                if formats.len() != profile.formats.len() || features.len() != profile.features.len() {
                    return Err(ResolveError::InvalidRegistry("duplicate format or feature"));
                }
                if (profile.operation == RenderOperation::Image) == profile.formats.is_empty() {
                    return Err(ResolveError::InvalidRegistry("image profiles require formats; viewport profiles must have none"));
                }
            }
        }
    }
    Ok(())
}

fn check_limit(resource: &'static str, requested: u64, limit: u64) -> Result<(), ResolveError> {
    if requested > limit { Err(ResolveError::LimitExceeded { resource, requested, limit }) } else { Ok(()) }
}

fn check_request(executor: &ExecutorDescriptor, profile: &RenderProfile, request: &RenderRequest) -> Result<u64, ResolveError> {
    match (request.operation, request.format) {
        (RenderOperation::Image, Some(format)) if profile.formats.contains(&format) => {}
        (RenderOperation::Viewport, None) => {}
        _ => return Err(ResolveError::UnsupportedFormat),
    }
    // Stable diagnostics even if callers reorder required features.
    for feature in request.features.iter().collect::<BTreeSet<_>>() {
        if !profile.features.contains(feature) {
            return Err(ResolveError::UnsupportedFeature(*feature));
        }
    }
    let limits = executor.limits;
    check_limit("width", request.width, limits.max_dimension)?;
    check_limit("height", request.height, limits.max_dimension)?;
    let pixels = request.width.checked_mul(request.height).ok_or(ResolveError::SizeOverflow)?;
    check_limit("pixels", pixels, limits.max_pixels)?;
    let scratch = executor.scratch.estimate(request.width, request.height)?;
    check_limit("estimated scratch bytes", scratch, limits.max_scratch_bytes)?;
    Ok(scratch)
}

/// Selects the lexicographically first available executor that satisfies the complete request.
/// Order in the registry never implies priority; explicit backends never fall back to another.
pub fn resolve_request(backends: &[BackendDescriptor], request: &RenderRequest) -> Result<ResolvedRequest, ResolveError> {
    validate_registry(backends)?;
    if request.width == 0 || request.height == 0 {
        return Err(ResolveError::InvalidDimensions);
    }
    if request.features.len() > 32 {
        return Err(ResolveError::InvalidRequest("too many required features (maximum 32)"));
    }
    let id = request.backend.clone().unwrap_or_else(|| BackendId("builtin".into()));
    let backend = backends.iter().find(|b| b.id == id).ok_or_else(|| ResolveError::UnknownBackend(id.clone()))?;
    if let Availability::Unavailable { reason } = &backend.availability {
        return Err(ResolveError::BackendUnavailable(reason.clone()));
    }
    let mut executors: Vec<_> = backend.executors.iter().filter(|e| e.availability == Availability::Available).collect();
    executors.sort_by(|a, b| a.id.cmp(&b.id));
    if executors.is_empty() {
        return Err(ResolveError::NoAvailableExecutor);
    }
    let mut failure = None;
    for executor in executors {
        let Some(profile) = executor.profiles.iter().find(|p| p.mode == request.mode && p.operation == request.operation) else { continue };
        match check_request(executor, profile, request) {
            Ok(estimated_scratch_bytes) => {
                let mut profile = profile.clone();
                profile.formats.sort();
                profile.features.sort();
                return Ok(ResolvedRequest {
                    backend: id,
                    executor: executor.id.clone(),
                    profile,
                    width: request.width,
                    height: request.height,
                    format: request.format,
                    estimated_scratch_bytes,
                    warnings: Vec::new(),
                });
            }
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    Err(failure.unwrap_or(ResolveError::UnsupportedProfile { mode: request.mode, operation: request.operation }))
}

#[cfg(test)]
mod tests;
