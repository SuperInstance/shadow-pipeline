//! # shadow-pipeline
//!
//! Lossy shadow compression pipeline with ordered stages, fidelity scoring,
//! and human-readable rendering.

use std::fmt;

// ── Core Types ──────────────────────────────────────────────────────────────

/// Compression level from 0 (raw) to 10 (max compression / most lossy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompressionLevel(pub u8);

impl CompressionLevel {
    pub const RAW: CompressionLevel = CompressionLevel(0);
    pub const LIGHT: CompressionLevel = CompressionLevel(3);
    pub const MEDIUM: CompressionLevel = CompressionLevel(5);
    pub const HEAVY: CompressionLevel = CompressionLevel(8);
    pub const MAX: CompressionLevel = CompressionLevel(10);
}

/// Metadata attached to shadow data.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ShadowMetadata {
    pub source: String,
    pub timestamp: u64,
    pub tags: Vec<String>,
}



/// Shadow data: raw bytes with metadata and compression tracking.
#[derive(Debug, Clone)]
pub struct ShadowData {
    pub bytes: Vec<u8>,
    pub metadata: ShadowMetadata,
    pub compression_level: CompressionLevel,
}

impl ShadowData {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            metadata: ShadowMetadata::default(),
            compression_level: CompressionLevel::RAW,
        }
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.metadata.source = source.to_string();
        self
    }

    pub fn with_tags(mut self, tags: Vec<&str>) -> Self {
        self.metadata.tags = tags.into_iter().map(String::from).collect();
        self
    }

    pub fn with_compression(mut self, level: CompressionLevel) -> Self {
        self.compression_level = level;
        self
    }

    pub fn byte_count(&self) -> usize {
        self.bytes.len()
    }

    /// Compute a simple signal metric: number of non-zero bytes.
    pub fn signal_strength(&self) -> usize {
        self.bytes.iter().filter(|&&b| b != 0).count()
    }
}

// ── Compression Metrics ─────────────────────────────────────────────────────

/// Metrics about compression performance.
#[derive(Debug, Clone, Copy)]
pub struct CompressionMetrics {
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f64,
    pub fidelity_score: f64,
}

impl CompressionMetrics {
    pub fn new(original: usize, compressed: usize) -> Self {
        let ratio = if original > 0 {
            compressed as f64 / original as f64
        } else {
            1.0
        };
        let fidelity = 1.0 - ratio.min(1.0); // simplified: less compression = higher fidelity
        Self {
            original_size: original,
            compressed_size: compressed,
            ratio,
            fidelity_score: fidelity,
        }
    }

    pub fn is_lossless(&self) -> bool {
        self.compressed_size == self.original_size
    }
}

impl fmt::Display for CompressionMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CompressionMetrics(original={}, compressed={}, ratio={:.2}, fidelity={:.2})",
            self.original_size, self.compressed_size, self.ratio, self.fidelity_score
        )
    }
}

// ── Shadow Stage Trait ──────────────────────────────────────────────────────

/// A transformation stage in the shadow compression pipeline.
pub trait ShadowStage: Send + Sync {
    /// Human-readable name of this stage.
    fn name(&self) -> &'static str;

    /// Transform shadow data, returning a new compressed representation.
    fn process(&self, data: ShadowData) -> ShadowData;
}

// ── Built-in Stages ─────────────────────────────────────────────────────────

/// Filter out low-signal (zero or near-zero) bytes.
pub struct FilterNoise {
    pub threshold: u8,
}

impl FilterNoise {
    pub fn new(threshold: u8) -> Self {
        Self { threshold }
    }
}

impl Default for FilterNoise {
    fn default() -> Self {
        Self::new(5)
    }
}

impl ShadowStage for FilterNoise {
    fn name(&self) -> &'static str { "FilterNoise" }
    fn process(&self, data: ShadowData) -> ShadowData {
        let filtered: Vec<u8> = data.bytes
            .into_iter()
            .filter(|&b| b >= self.threshold)
            .collect();
        ShadowData {
            bytes: filtered,
            metadata: data.metadata,
            compression_level: CompressionLevel(data.compression_level.0.saturating_add(1)),
        }
    }
}

/// Reduce precision by quantizing byte values into buckets.
pub struct Quantize {
    pub bucket_size: u8,
}

impl Quantize {
    pub fn new(bucket_size: u8) -> Self {
        Self { bucket_size: std::cmp::max(bucket_size, 1) }
    }
}

impl Default for Quantize {
    fn default() -> Self {
        Self::new(16)
    }
}

impl ShadowStage for Quantize {
    fn name(&self) -> &'static str { "Quantize" }
    fn process(&self, data: ShadowData) -> ShadowData {
        let quantized: Vec<u8> = data.bytes
            .into_iter()
            .map(|b| (b / self.bucket_size) * self.bucket_size)
            .collect();
        ShadowData {
            bytes: quantized,
            metadata: data.metadata,
            compression_level: data.compression_level,
        }
    }
}

/// Extract key metrics summary from the data, producing a compact form.
pub struct Summarize;

impl ShadowStage for Summarize {
    fn name(&self) -> &'static str { "Summarize" }
    fn process(&self, data: ShadowData) -> ShadowData {
        let len = data.bytes.len() as u8;
        let min = data.bytes.iter().copied().min().unwrap_or(0);
        let max = data.bytes.iter().copied().max().unwrap_or(0);
        let avg = if !data.bytes.is_empty() {
            (data.bytes.iter().map(|&b| b as u32).sum::<u32>() / data.bytes.len() as u32) as u8
        } else {
            0
        };
        let summary = vec![len, min, max, avg];
        ShadowData {
            bytes: summary,
            metadata: data.metadata,
            compression_level: CompressionLevel::HEAVY,
        }
    }
}

/// Render into a human-readable string form.
pub struct Render;

impl ShadowStage for Render {
    fn name(&self) -> &'static str { "Render" }
    fn process(&self, data: ShadowData) -> ShadowData {
        let rendered = format!(
            "[{}] {} bytes, level={}, signal={}",
            data.metadata.source,
            data.bytes.len(),
            data.compression_level.0,
            data.signal_strength(),
        );
        ShadowData {
            bytes: rendered.into_bytes(),
            metadata: data.metadata,
            compression_level: data.compression_level,
        }
    }
}

// ── Pipeline ────────────────────────────────────────────────────────────────

/// Ordered pipeline of shadow compression stages.
pub struct Pipeline {
    stages: Vec<Box<dyn ShadowStage>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn add_stage(mut self, stage: impl ShadowStage + 'static) -> Self {
        self.stages.push(Box::new(stage));
        self
    }

    /// Run all stages in order, tracking metrics.
    pub fn execute(&self, mut data: ShadowData) -> (ShadowData, Vec<CompressionMetrics>) {
        let original_size = data.byte_count();
        let mut metrics = Vec::new();
        for stage in &self.stages {
            let pre_size = data.byte_count();
            data = stage.process(data);
            let post_size = data.byte_count();
            metrics.push(CompressionMetrics::new(pre_size, post_size));
        }
        let final_metrics = CompressionMetrics::new(original_size, data.byte_count());
        metrics.push(final_metrics);
        (data, metrics)
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub fn stage_names(&self) -> Vec<&'static str> {
        self.stages.iter().map(|s| s.name()).collect()
    }
}

impl Default for Pipeline {
    fn default() -> Self { Self::new() }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> ShadowData {
        ShadowData::new(vec![10, 0, 20, 3, 50, 0, 0, 80, 100, 2])
            .with_source("test")
            .with_tags(vec!["sample"])
    }

    #[test]
    fn shadow_data_byte_count() {
        let d = sample_data();
        assert_eq!(d.byte_count(), 10);
    }

    #[test]
    fn shadow_data_signal_strength() {
        let d = sample_data();
        assert_eq!(d.signal_strength(), 7); // 3 zeros
    }

    #[test]
    fn filter_noise_removes_low_signal() {
        let stage = FilterNoise::new(5);
        let result = stage.process(sample_data());
        // bytes below 5 removed: 0, 3, 0, 0, 2 → kept: 10, 20, 50, 80, 100
        assert_eq!(result.bytes, vec![10, 20, 50, 80, 100]);
    }

    #[test]
    fn filter_noise_increases_compression_level() {
        let stage = FilterNoise::default();
        let result = stage.process(sample_data());
        assert!(result.compression_level.0 > CompressionLevel::RAW.0);
    }

    #[test]
    fn quantize_buckets_values() {
        let stage = Quantize::new(16);
        let data = ShadowData::new(vec![10, 20, 35, 50, 63, 80, 100]);
        let result = stage.process(data);
        assert_eq!(result.bytes, vec![0, 16, 32, 48, 48, 80, 96]);
    }

    #[test]
    fn summarize_produces_compact_form() {
        let stage = Summarize;
        let data = ShadowData::new(vec![10, 20, 30, 40, 50]);
        let result = stage.process(data);
        assert_eq!(result.bytes.len(), 4); // len, min, max, avg
        assert_eq!(result.bytes[0], 5);  // len
        assert_eq!(result.bytes[1], 10); // min
        assert_eq!(result.bytes[2], 50); // max
    }

    #[test]
    fn render_produces_readable_bytes() {
        let stage = Render;
        let data = ShadowData::new(vec![1, 2, 3]).with_source("src");
        let result = stage.process(data);
        let s = String::from_utf8(result.bytes).unwrap();
        assert!(s.contains("src"));
        assert!(s.contains("3 bytes"));
    }

    #[test]
    fn pipeline_single_stage() {
        let p = Pipeline::new().add_stage(FilterNoise::new(10));
        let (result, metrics) = p.execute(sample_data());
        assert_eq!(result.bytes, vec![10, 20, 50, 80, 100]);
        assert_eq!(metrics.len(), 2); // 1 stage + 1 final
    }

    #[test]
    fn pipeline_multi_stage() {
        let p = Pipeline::new()
            .add_stage(FilterNoise::new(5))
            .add_stage(Quantize::new(16))
            .add_stage(Render);
        let (result, metrics) = p.execute(sample_data());
        assert_eq!(p.stage_names(), vec!["FilterNoise", "Quantize", "Render"]);
        let s = String::from_utf8(result.bytes).unwrap();
        assert!(!s.is_empty());
        assert_eq!(metrics.len(), 4); // 3 stages + 1 final
    }

    #[test]
    fn compression_metrics_lossless() {
        let m = CompressionMetrics::new(100, 100);
        assert!(m.is_lossless());
        assert_eq!(m.fidelity_score, 0.0); // no compression = no fidelity loss in this model
    }

    #[test]
    fn compression_metrics_display() {
        let m = CompressionMetrics::new(100, 50);
        let s = format!("{}", m);
        assert!(s.contains("ratio=0.50"));
        assert!(s.contains("fidelity=0.50"));
    }

    #[test]
    fn pipeline_empty_data() {
        let p = Pipeline::new().add_stage(Summarize);
        let data = ShadowData::new(vec![]);
        let (result, _) = p.execute(data);
        assert_eq!(result.bytes[0], 0); // len=0, min=0, max=0, avg=0
    }

    #[test]
    fn pipeline_stage_count() {
        let p = Pipeline::new()
            .add_stage(FilterNoise::default())
            .add_stage(Quantize::default());
        assert_eq!(p.stage_count(), 2);
    }
}
