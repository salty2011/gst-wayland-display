use serde::{Deserialize, Serialize};
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HdrMetadata {
    pub eotf: Eotf,
    pub mastering_display_info: Option<MasteringDisplayInfo>,
    pub content_light_level: Option<ContentLightLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Eotf {
    Traditional,
    Srgb,
    St2084,  // HDR10/PQ
    Hlg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringDisplayInfo {
    pub display_primaries: [DisplayPrimary; 3],
    pub white_point: ChromaticityCoordinate,
    pub max_display_mastering_luminance: u32,
    pub min_display_mastering_luminance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayPrimary {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaticityCoordinate {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentLightLevel {
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}

pub struct HdrState {
    pub metadata: Option<HdrMetadata>,
    pub supported_eotfs: Vec<Eotf>,
    pub max_luminance: u32,
    pub min_luminance: u32,
}

impl Default for HdrState {
    fn default() -> Self {
        Self {
            metadata: None,
            supported_eotfs: vec![Eotf::Traditional, Eotf::Srgb],
            max_luminance: 100, // Default SDR luminance
            min_luminance: 0,
        }
    }
}

impl HdrState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_metadata(&mut self, metadata: HdrMetadata) {
        self.metadata = Some(metadata);
    }

    pub fn supports_hdr(&self) -> bool {
        self.supported_eotfs.iter().any(|eotf| matches!(eotf, Eotf::St2084 | Eotf::Hlg))
    }
} 