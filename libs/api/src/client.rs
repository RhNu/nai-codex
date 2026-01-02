use reqwest::{Client, header};
use serde_json::{Value, json};

use crate::{
    error::{NaiError, NaiResult},
    types::{Action, ImageGenerationRequest, Sampler},
    util::{extract_file_by_name, normalize_seed},
};

#[derive(Debug, Clone)]
pub struct NaiClient {
    client: Client,
    token: String,
}

impl NaiClient {
    pub fn new(token: String) -> NaiResult<Self> {
        let token = token
            .trim()
            .trim_matches('"')
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .unwrap_or(token.as_str())
            .to_string();

        let mut headers = header::HeaderMap::new();

        headers.insert(header::ACCEPT, header::HeaderValue::from_static("*/*"));
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::ORIGIN,
            header::HeaderValue::from_static("https://novelai.net"),
        );
        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://novelai.net/"),
        );

        Ok(Self {
            client: Client::builder().default_headers(headers).build()?,
            token,
        })
    }

    async fn post(&self, url: &str, payload: &Value) -> NaiResult<Vec<u8>> {
        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(payload)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.bytes().await?;

        if status.is_success() {
            Ok(body.to_vec())
        } else {
            Err(NaiError::BadStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).to_string(),
            })
        }
    }

    async fn post_generate_image(&self, payload: &Value) -> NaiResult<Vec<u8>> {
        self.post("https://image.novelai.net/ai/generate-image", payload)
            .await
    }

    async fn post_argument_image(&self, payload: &Value) -> NaiResult<Vec<u8>> {
        self.post("https://image.novelai.net/ai/argument-image", payload)
            .await
    }

    pub async fn generate_image(&self, req: &ImageGenerationRequest) -> NaiResult<Vec<u8>> {
        let seed = normalize_seed(req.seed.unwrap_or(-1));
        let uc_preset_id = req.uc_preset_id();
        let use_coords = req.need_use_coords();
        let prompt = if req.add_quality_tags {
            format!("{}{}", req.prompt_positive, req.model.quality_tags())
        } else {
            req.prompt_positive.clone()
        };

        let mut payload = json!({
            "input": prompt,
            "model": req.model,
            "action": Action::Generate,
            "parameters": {
                "params_version": 3,
                "width": req.width,
                "height": req.height,
                "scale": req.scale,
                "sampler": req.sampler,
                "steps": req.steps,
                "n_samples": 1,
                "ucPreset": uc_preset_id,
                "qualityToggle": req.add_quality_tags,
                "autoSmea": false,
                "dynamic_thresholding": false,
                "legacy": false,
                "legacy_v3_extend": false,
                "add_original_image": true,
                "seed": seed,
                "negative_prompt": req.prompt_negative,
                "cfg_rescale": req.cfg_rescale,
                "noise_schedule": req.noise,
                "autoSmea": false,
                "legacy": false,
                "dynamic_thresholding": false,
                "stream": "msgpack"
            },
            "use_new_shared_trial": true,
        });

        let enabled_chars = req
            .character_prompts
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.enabled)
            .collect::<Vec<_>>();
        let char_positive = enabled_chars
            .iter()
            .map(|c| {
                json!({
                    "char_caption": c.prompt,
                    "centers": [{"x": c.center.x, "y": c.center.y}]
                })
            })
            .collect::<Vec<_>>();
        let char_negative = enabled_chars
            .iter()
            .map(|c| {
                json!({
                    "char_caption": c.uc,
                    "centers": [{"x": c.center.x, "y": c.center.y}]
                })
            })
            .collect::<Vec<_>>();

        payload["parameters"]["use_coords"] = json!(req.need_use_coords());
        payload["parameters"]["characterPrompts"] = json!(enabled_chars);
        payload["parameters"]["v4_prompt"] = json!({
            "caption": {
                "base_caption": prompt,
                "char_captions": char_positive
            },
            "use_coords": use_coords,
            "use_order": true
        });
        payload["parameters"]["v4_negative_prompt"] = json!({
            "caption": {
                "base_caption": req.prompt_negative,
                "char_captions": char_negative
            },
            "legacy_uc": false
        });

        if req.sampler == Sampler::EulerAncestral {
            payload["parameters"]["deliberate_euler_ancestral_bug"] = json!(false);
            payload["parameters"]["prefer_brownian"] = json!(true);
        }

        if req.variety_plus {
            payload["parameters"]["skip_cfg_above_sigma"] = json!(req.model.skip_cfg_above_sigma());
        }

        let bytes = self.post_generate_image(&payload).await?;
        let image = extract_file_by_name(&bytes, "image_0.png").ok_or(NaiError::BadResult {
            file_name: "image_0.png".to_string(),
        })?;

        Ok(image)
    }
}
