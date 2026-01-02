use crate::util::default_true;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Model {
    #[default]
    #[serde(rename = "nai-diffusion-4-5-full")]
    V45Full,
    #[serde(rename = "nai-diffusion-4-5-curated")]
    V45Curated,
}

impl Model {
    pub const fn quality_tags(&self) -> &'static str {
        match self {
            Self::V45Full => ", very aesthetic, masterpiece, no text",
            Self::V45Curated => {
                ", very aesthetic, masterpiece, no text, -0.8::feet::, rating:general"
            }
        }
    }

    pub const fn skip_cfg_above_sigma(&self) -> f32 {
        match self {
            Self::V45Full => 58.0,
            Self::V45Curated => 36.158_893_609_242_725,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Sampler {
    #[serde(rename = "k_euler")]
    Euler,
    #[default]
    #[serde(rename = "k_euler_ancestral")]
    EulerAncestral,
    #[serde(rename = "k_dpmpp_2s_ancestral")]
    Dpm2sAncestral,
    #[serde(rename = "k_dpmpp_2m")]
    Dpm2m,
    #[serde(rename = "k_dpmpp_sde")]
    DpmSde,
    #[serde(rename = "k_dpmpp_2m_sde")]
    Dpm2mSde,
    #[serde(rename = "ddim_v3")]
    DdimV3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Noise {
    #[serde(rename = "native")]
    Native,
    #[default]
    #[serde(rename = "karras")]
    Karras,
    #[serde(rename = "exponential")]
    Exponential,
    #[serde(rename = "polyexponential")]
    PolyExponential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    /// Model to use for image generation
    #[serde(default)]
    pub model: Model,
    /// Main prompt for image generation
    #[serde(default)]
    pub prompt_positive: String,
    /// UC stands for "Unwanted Content"
    #[serde(default)]
    pub prompt_negative: String,
    /// Number of images to generate for a single submission. Defaults to 1 if not specified.
    #[serde(default)]
    pub quantity: Option<u32>,

    pub width: u32,
    pub height: u32,

    #[serde(default = "default_steps")]
    pub steps: u32,
    #[serde(default = "defualt_scale")]
    pub scale: f32,

    #[serde(default)]
    pub sampler: Sampler,
    #[serde(default)]
    pub noise: Noise,

    /// Variety Plus mode
    #[serde(default)]
    pub variety_plus: bool,

    /// CFG Rescale value; Defaults to 0.0 if not specified
    #[serde(default)]
    pub cfg_rescale: f32,

    /// Seed, negative or None for random seed
    #[serde(default)]
    pub seed: Option<i64>,

    /// Character prompts
    #[serde(default)]
    pub character_prompts: Option<Vec<CharacterPrompt>>,

    /// Preset options
    #[serde(default = "default_true")]
    pub add_quality_tags: bool,
    #[serde(default)]
    pub undesired_content_preset: Option<u8>,

    /// Use legacy UC method; Should be false
    #[serde(default)]
    pub legacy_uc: bool,
}

impl ImageGenerationRequest {
    pub fn uc_preset_id(&self) -> u8 {
        match self.model {
            // 0-4 are valid for V4.5 Full models
            // 0: Heavy, 1: Light, 2: Furry Focus, 3: Human Focus, 4: None
            Model::V45Full => self
                .undesired_content_preset
                .map(|id| id.min(4))
                .unwrap_or(4),
            // 0-3 are valid for V4.5 Curated models
            // 0: Heavy, 1: Light, 2: Human Focus, 3: None
            Model::V45Curated => self
                .undesired_content_preset
                .map(|id| id.min(3))
                .unwrap_or(3),
        }
    }

    pub fn need_use_coords(&self) -> bool {
        if let Some(chars) = &self.character_prompts {
            if chars.is_empty() {
                false
            } else {
                chars
                    .iter()
                    .any(|char| char.enabled && (char.center.x != 0.5 || char.center.y != 0.5))
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPrompt {
    pub prompt: String,
    pub uc: String,
    #[serde(default)]
    pub center: Center,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Center {
    pub x: f32,
    pub y: f32,
}

impl Default for Center {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    #[serde(rename = "generate")]
    Generate,
}

fn default_steps() -> u32 {
    28
}

fn defualt_scale() -> f32 {
    5.0
}
