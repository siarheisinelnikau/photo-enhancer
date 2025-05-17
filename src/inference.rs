//! Extension boundary for optional, user-installed local inference backends.
//! No runtime, weights, network inference, or fabricated output is bundled.
use crate::engine::Adjustments;
use anyhow::Result;
use image::RgbaImage;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Capability {
    Unavailable {
        reason: String,
    },
    Ready {
        model: String,
        license: String,
        max_tile_edge: u32,
    },
}

pub trait LocalEnhancementBackend: Send {
    fn capability(&self) -> Capability;
    /// Implementations must use local resources, bounded tiles with overlap,
    /// preserve dimensions/alpha contracts, and observe cancellation per tile.
    fn enhance(
        &mut self,
        image: &RgbaImage,
        settings: &Adjustments,
        cancel: &AtomicBool,
    ) -> Result<RgbaImage>;
}

pub struct UnavailableBackend;
impl LocalEnhancementBackend for UnavailableBackend {
    fn capability(&self) -> Capability {
        Capability::Unavailable { reason: "Learned restoration is not installed. This build includes deterministic filters only.".into() }
    }
    fn enhance(&mut self, _: &RgbaImage, _: &Adjustments, _: &AtomicBool) -> Result<RgbaImage> {
        anyhow::bail!("No local inference runtime or verified model is installed")
    }
}
