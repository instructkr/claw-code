//! Provider adapters for the image-generation backends listed in
//! Spec §10.2: a local `ComfyUI` graph endpoint, a self-hosted `Diffusers`
//! service, and an internal image-render worker. Each adapter translates the
//! canonical request schema (which mirrors §4.1 / §4.5) into the wire format
//! that the provider actually accepts, and parses the response back into the
//! same canonical shape so the orchestrator and validator stage do not have
//! to reason about provider differences.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalControlInput {
    #[serde(rename = "type")]
    pub control_type: String,
    pub uri: String,
    pub strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CanonicalGenerationRequest {
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub seed: u64,
    pub sampler: String,
    pub model: String,
    #[serde(default)]
    pub batch_size: Option<u32>,
    #[serde(default)]
    pub style_preset: Option<String>,
    #[serde(default)]
    pub control_inputs: Vec<CanonicalControlInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalInpaintRequest {
    pub image_uri: String,
    pub mask_uri: String,
    pub prompt: String,
    pub negative_prompt: String,
    pub denoise_strength: f64,
    pub steps: u32,
    pub cfg: f64,
    pub seed: u64,
    pub model: String,
    #[serde(default)]
    pub preserve_edges: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratedImage {
    pub uri: String,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalGenerationResponse {
    pub images: Vec<GeneratedImage>,
    pub metadata: Value,
}

/// Result of `ImageProviderRegistry::translate_*` — exposes both the rewritten
/// payload and the resolved endpoint URL so callers can either dispatch via
/// `HttpInvoker` or hand the bundle to a different transport (worker channel,
/// queue, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderInvocation {
    pub provider: String,
    pub endpoint: String,
    pub payload: Value,
}

/// Adapter contract a backend implements.
pub trait ImageProvider: Send + Sync {
    fn id(&self) -> &'static str;

    /// Resolve the absolute URL the provider expects for a given operation,
    /// given the user-supplied base URL (e.g. `http://localhost:8188`).
    fn endpoint(&self, op: ImageOp, base_url: &str) -> String;

    /// Translate a canonical generate payload into the provider's wire format.
    fn translate_generate(&self, req: &CanonicalGenerationRequest) -> Value;

    /// Translate a canonical inpaint payload into the provider's wire format.
    fn translate_inpaint(&self, req: &CanonicalInpaintRequest) -> Value;

    /// Parse a provider response into the canonical generation response.
    /// Inpaint responses share the schema (single image with metadata).
    fn parse_response(&self, raw: &Value) -> Result<CanonicalGenerationResponse, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageOp {
    Generate,
    Inpaint,
}

/// Implementation of the local `ComfyUI` graph endpoint adapter.
///
/// `ComfyUI` accepts a queued workflow under `/prompt`; we wrap the canonical
/// fields in the minimal nodes the harness needs (`KSampler`, latent image,
/// model loader, conditioning, VAE decode) so that the backend can hydrate
/// it into a usable graph. The outputs are reported under `/history/{id}` as
/// `images: [{ filename, subfolder, type }]` which we normalise into URIs.
#[derive(Debug, Default, Clone)]
pub struct ComfyUiProvider;

impl ComfyUiProvider {
    fn build_workflow(req: &CanonicalGenerationRequest) -> Value {
        let mut workflow = serde_json::Map::new();
        workflow.insert(
            "3".to_string(),
            json!({
                "class_type": "KSampler",
                "inputs": {
                    "seed": req.seed,
                    "steps": req.steps,
                    "cfg": req.cfg,
                    "sampler_name": req.sampler,
                    "scheduler": "karras",
                    "denoise": 1.0,
                    "model": ["4", 0],
                    "positive": ["6", 0],
                    "negative": ["7", 0],
                    "latent_image": ["5", 0]
                }
            }),
        );
        workflow.insert(
            "4".to_string(),
            json!({
                "class_type": "CheckpointLoaderSimple",
                "inputs": { "ckpt_name": req.model }
            }),
        );
        workflow.insert(
            "5".to_string(),
            json!({
                "class_type": "EmptyLatentImage",
                "inputs": {
                    "width": req.width,
                    "height": req.height,
                    "batch_size": req.batch_size.unwrap_or(1)
                }
            }),
        );
        workflow.insert(
            "6".to_string(),
            json!({
                "class_type": "CLIPTextEncode",
                "inputs": { "text": req.prompt, "clip": ["4", 1] }
            }),
        );
        workflow.insert(
            "7".to_string(),
            json!({
                "class_type": "CLIPTextEncode",
                "inputs": { "text": req.negative_prompt, "clip": ["4", 1] }
            }),
        );
        workflow.insert(
            "8".to_string(),
            json!({
                "class_type": "VAEDecode",
                "inputs": { "samples": ["3", 0], "vae": ["4", 2] }
            }),
        );
        workflow.insert(
            "9".to_string(),
            json!({
                "class_type": "SaveImage",
                "inputs": { "filename_prefix": "iqh", "images": ["8", 0] }
            }),
        );
        Value::Object(workflow)
    }

    fn build_inpaint_workflow(req: &CanonicalInpaintRequest) -> Value {
        json!({
            "3": {
                "class_type": "KSampler",
                "inputs": {
                    "seed": req.seed,
                    "steps": req.steps,
                    "cfg": req.cfg,
                    "sampler_name": "dpmpp_2m",
                    "scheduler": "karras",
                    "denoise": req.denoise_strength,
                    "model": ["4", 0],
                    "positive": ["6", 0],
                    "negative": ["7", 0],
                    "latent_image": ["10", 0]
                }
            },
            "4": {
                "class_type": "CheckpointLoaderSimple",
                "inputs": { "ckpt_name": req.model }
            },
            "6": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": req.prompt, "clip": ["4", 1] }
            },
            "7": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": req.negative_prompt, "clip": ["4", 1] }
            },
            "8": {
                "class_type": "LoadImage",
                "inputs": { "image": req.image_uri }
            },
            "9": {
                "class_type": "LoadImageMask",
                "inputs": { "image": req.mask_uri, "channel": "red" }
            },
            "10": {
                "class_type": "VAEEncodeForInpaint",
                "inputs": {
                    "pixels": ["8", 0],
                    "vae": ["4", 2],
                    "mask": ["9", 0],
                    "grow_mask_by": if req.preserve_edges.unwrap_or(true) { 12 } else { 0 }
                }
            },
            "11": {
                "class_type": "VAEDecode",
                "inputs": { "samples": ["3", 0], "vae": ["4", 2] }
            },
            "12": {
                "class_type": "SaveImage",
                "inputs": { "filename_prefix": "iqh-inpaint", "images": ["11", 0] }
            }
        })
    }
}

impl ImageProvider for ComfyUiProvider {
    fn id(&self) -> &'static str {
        "comfyui"
    }

    fn endpoint(&self, _op: ImageOp, base_url: &str) -> String {
        format!("{}/prompt", base_url.trim_end_matches('/'))
    }

    fn translate_generate(&self, req: &CanonicalGenerationRequest) -> Value {
        json!({
            "client_id": "rusty-harness",
            "prompt": Self::build_workflow(req)
        })
    }

    fn translate_inpaint(&self, req: &CanonicalInpaintRequest) -> Value {
        json!({
            "client_id": "rusty-harness",
            "prompt": Self::build_inpaint_workflow(req)
        })
    }

    fn parse_response(&self, raw: &Value) -> Result<CanonicalGenerationResponse, String> {
        // ComfyUI returns either {"prompt_id": "...", "outputs": {...}} or
        // a raw history entry. Handle both. Each output node lists `images`
        // with `filename`, `subfolder`, and `type` ("output" or "temp").
        let outputs = raw
            .get("outputs")
            .or_else(|| raw.pointer("/data/outputs"))
            .ok_or("ComfyUI response missing `outputs`")?;
        let outputs_obj = outputs
            .as_object()
            .ok_or("ComfyUI `outputs` must be an object")?;
        let mut images = Vec::new();
        for (_node_id, node) in outputs_obj {
            let Some(img_array) = node.get("images").and_then(Value::as_array) else {
                continue;
            };
            for img in img_array {
                let filename = img
                    .get("filename")
                    .and_then(Value::as_str)
                    .ok_or("ComfyUI image missing `filename`")?;
                let subfolder = img.get("subfolder").and_then(Value::as_str).unwrap_or("");
                let kind = img.get("type").and_then(Value::as_str).unwrap_or("output");
                let uri = if subfolder.is_empty() {
                    format!("comfyui://{kind}/{filename}")
                } else {
                    format!("comfyui://{kind}/{subfolder}/{filename}")
                };
                let seed = img.get("seed").and_then(Value::as_u64).unwrap_or(0);
                images.push(GeneratedImage { uri, seed });
            }
        }
        if images.is_empty() {
            return Err("ComfyUI response contained no images".to_string());
        }
        let metadata = json!({
            "prompt_id": raw.get("prompt_id"),
            "node_count": outputs_obj.len()
        });
        Ok(CanonicalGenerationResponse { images, metadata })
    }
}

/// Self-hosted Diffusers REST service adapter.
///
/// Targets the canonical Diffusers endpoint contract (`/v1/text-to-image` and
/// `/v1/inpaint`) where the body matches the SDXL pipeline kwargs. Outputs are
/// returned as `{"artifacts": [{"uri": ..., "seed": ...}]}` or, for the
/// streaming-style variant, `{"images": [{"url"|"uri", "seed"}]}` — both are
/// accepted.
#[derive(Debug, Default, Clone)]
pub struct DiffusersProvider;

impl ImageProvider for DiffusersProvider {
    fn id(&self) -> &'static str {
        "diffusers"
    }

    fn endpoint(&self, op: ImageOp, base_url: &str) -> String {
        let trimmed = base_url.trim_end_matches('/');
        match op {
            ImageOp::Generate => format!("{trimmed}/v1/text-to-image"),
            ImageOp::Inpaint => format!("{trimmed}/v1/inpaint"),
        }
    }

    fn translate_generate(&self, req: &CanonicalGenerationRequest) -> Value {
        let mut payload = json!({
            "model": req.model,
            "prompt": req.prompt,
            "negative_prompt": req.negative_prompt,
            "width": req.width,
            "height": req.height,
            "num_inference_steps": req.steps,
            "guidance_scale": req.cfg,
            "scheduler": req.sampler,
            "seed": req.seed,
            "num_images_per_prompt": req.batch_size.unwrap_or(1),
        });
        if let Some(preset) = &req.style_preset {
            payload["style_preset"] = json!(preset);
        }
        if !req.control_inputs.is_empty() {
            payload["controlnet"] =
                serde_json::to_value(&req.control_inputs).unwrap_or(Value::Null);
        }
        payload
    }

    fn translate_inpaint(&self, req: &CanonicalInpaintRequest) -> Value {
        json!({
            "model": req.model,
            "prompt": req.prompt,
            "negative_prompt": req.negative_prompt,
            "image": req.image_uri,
            "mask_image": req.mask_uri,
            "num_inference_steps": req.steps,
            "guidance_scale": req.cfg,
            "strength": req.denoise_strength,
            "seed": req.seed,
            "preserve_edges": req.preserve_edges.unwrap_or(true)
        })
    }

    fn parse_response(&self, raw: &Value) -> Result<CanonicalGenerationResponse, String> {
        let array = raw
            .get("artifacts")
            .or_else(|| raw.get("images"))
            .and_then(Value::as_array)
            .ok_or("Diffusers response missing `artifacts`/`images`")?;
        let mut images = Vec::with_capacity(array.len());
        for entry in array {
            let uri = entry
                .get("uri")
                .or_else(|| entry.get("url"))
                .or_else(|| entry.get("image"))
                .and_then(Value::as_str)
                .ok_or("Diffusers image entry missing `uri`/`url`")?;
            let seed = entry.get("seed").and_then(Value::as_u64).unwrap_or(0);
            images.push(GeneratedImage {
                uri: uri.to_string(),
                seed,
            });
        }
        if images.is_empty() {
            return Err("Diffusers response contained no images".to_string());
        }
        let metadata = raw
            .get("metadata")
            .cloned()
            .unwrap_or_else(|| json!({"provider": "diffusers"}));
        Ok(CanonicalGenerationResponse { images, metadata })
    }
}

/// Internal image-render worker adapter.
///
/// Speaks to a queue-style internal service that exposes `/jobs/generate`
/// and `/jobs/inpaint` and returns `{"job_id": ..., "result": {...}}`. The
/// canonical fields are wrapped in a typed `task` envelope so routing
/// metadata can be injected without disturbing the harness contract.
#[derive(Debug, Default, Clone)]
pub struct InternalWorkerProvider;

impl ImageProvider for InternalWorkerProvider {
    fn id(&self) -> &'static str {
        "internal_worker"
    }

    fn endpoint(&self, op: ImageOp, base_url: &str) -> String {
        let trimmed = base_url.trim_end_matches('/');
        match op {
            ImageOp::Generate => format!("{trimmed}/jobs/generate"),
            ImageOp::Inpaint => format!("{trimmed}/jobs/inpaint"),
        }
    }

    fn translate_generate(&self, req: &CanonicalGenerationRequest) -> Value {
        json!({
            "task": "generate_image",
            "submitter": "rusty-harness",
            "params": serde_json::to_value(req).unwrap_or(Value::Null)
        })
    }

    fn translate_inpaint(&self, req: &CanonicalInpaintRequest) -> Value {
        json!({
            "task": "inpaint_region",
            "submitter": "rusty-harness",
            "params": serde_json::to_value(req).unwrap_or(Value::Null)
        })
    }

    fn parse_response(&self, raw: &Value) -> Result<CanonicalGenerationResponse, String> {
        let result = raw
            .get("result")
            .ok_or("internal worker response missing `result`")?;
        let images_value = result
            .get("images")
            .or_else(|| result.get("outputs"))
            .ok_or("internal worker `result` missing `images`")?;
        let images_array = images_value
            .as_array()
            .ok_or("internal worker `images` must be an array")?;
        let mut images = Vec::with_capacity(images_array.len());
        for entry in images_array {
            let uri = entry
                .get("uri")
                .or_else(|| entry.get("path"))
                .and_then(Value::as_str)
                .ok_or("internal worker image entry missing `uri`/`path`")?;
            let seed = entry.get("seed").and_then(Value::as_u64).unwrap_or(0);
            images.push(GeneratedImage {
                uri: uri.to_string(),
                seed,
            });
        }
        if images.is_empty() {
            return Err("internal worker response contained no images".to_string());
        }
        let metadata = json!({
            "job_id": raw.get("job_id"),
            "queue": raw.get("queue"),
            "duration_ms": raw.get("duration_ms")
        });
        Ok(CanonicalGenerationResponse { images, metadata })
    }
}

/// Registry of provider adapters, keyed by the `provider` string passed to
/// the runtime tool.
#[derive(Clone)]
pub struct ImageProviderRegistry {
    providers: BTreeMap<String, Arc<dyn ImageProvider>>,
}

impl std::fmt::Debug for ImageProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for ImageProviderRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ImageProviderRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self {
            providers: BTreeMap::new(),
        };
        registry.register(Arc::new(ComfyUiProvider));
        registry.register(Arc::new(DiffusersProvider));
        registry.register(Arc::new(InternalWorkerProvider));
        registry
    }

    pub fn register(&mut self, provider: Arc<dyn ImageProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn ImageProvider>, String> {
        self.providers
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown image provider `{id}` (known: {})", self.list()))
    }

    fn list(&self) -> String {
        self.providers
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn translate_generate(
        &self,
        provider: &str,
        base_url: &str,
        req: &CanonicalGenerationRequest,
    ) -> Result<ProviderInvocation, String> {
        let adapter = self.get(provider)?;
        Ok(ProviderInvocation {
            provider: provider.to_string(),
            endpoint: adapter.endpoint(ImageOp::Generate, base_url),
            payload: adapter.translate_generate(req),
        })
    }

    pub fn translate_inpaint(
        &self,
        provider: &str,
        base_url: &str,
        req: &CanonicalInpaintRequest,
    ) -> Result<ProviderInvocation, String> {
        let adapter = self.get(provider)?;
        Ok(ProviderInvocation {
            provider: provider.to_string(),
            endpoint: adapter.endpoint(ImageOp::Inpaint, base_url),
            payload: adapter.translate_inpaint(req),
        })
    }

    pub fn parse_response(
        &self,
        provider: &str,
        raw: &Value,
    ) -> Result<CanonicalGenerationResponse, String> {
        self.get(provider)?.parse_response(raw)
    }
}

/// Trait for the HTTP layer the regression runner / tool execution uses.
/// Implemented by [`HttpInvoker`] for production and by mocked harness
/// servers in tests.
pub trait HttpInvoker: Send + Sync {
    fn post_json(&self, url: &str, payload: &Value) -> Result<Value, String>;
}

#[derive(Debug, Clone)]
pub struct ReqwestInvoker {
    timeout: Duration,
}

impl ReqwestInvoker {
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for ReqwestInvoker {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl HttpInvoker for ReqwestInvoker {
    fn post_json(&self, url: &str, payload: &Value) -> Result<Value, String> {
        let client = Client::new();
        let response = client
            .post(url)
            .timeout(self.timeout)
            .json(payload)
            .send()
            .map_err(|err| err.to_string())?;
        let status = response.status();
        let body = response.text().map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("backend HTTP {}: {body}", status.as_u16()));
        }
        serde_json::from_str(&body).map_err(|err| format!("backend response not JSON: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_generate_request() -> CanonicalGenerationRequest {
        CanonicalGenerationRequest {
            prompt: "knight in mirrored armor".to_string(),
            negative_prompt: "extra fingers".to_string(),
            width: 1024,
            height: 1024,
            steps: 30,
            cfg: 6.5,
            seed: 42,
            sampler: "dpmpp_2m".to_string(),
            model: "sdxl-base-1.0".to_string(),
            batch_size: Some(2),
            style_preset: Some("cinematic".to_string()),
            control_inputs: Vec::new(),
        }
    }

    fn sample_inpaint_request() -> CanonicalInpaintRequest {
        CanonicalInpaintRequest {
            image_uri: "img://1.png".to_string(),
            mask_uri: "img://1.mask.png".to_string(),
            prompt: "fix hand".to_string(),
            negative_prompt: "fused".to_string(),
            denoise_strength: 0.32,
            steps: 24,
            cfg: 6.0,
            seed: 11,
            model: "sdxl-inpaint".to_string(),
            preserve_edges: Some(true),
        }
    }

    #[test]
    fn comfyui_translates_canonical_generate_into_workflow_graph() {
        let provider = ComfyUiProvider;
        let req = sample_generate_request();
        let payload = provider.translate_generate(&req);
        assert_eq!(payload["client_id"], "rusty-harness");
        let workflow = payload["prompt"].as_object().expect("workflow obj");
        let ksampler = workflow.get("3").expect("ksampler node");
        assert_eq!(ksampler["class_type"], "KSampler");
        assert_eq!(ksampler["inputs"]["seed"], 42);
        assert_eq!(ksampler["inputs"]["steps"], 30);
        assert_eq!(workflow["4"]["inputs"]["ckpt_name"], "sdxl-base-1.0");
        assert_eq!(workflow["5"]["inputs"]["width"], 1024);
        assert_eq!(workflow["5"]["inputs"]["batch_size"], 2);
    }

    #[test]
    fn comfyui_inpaint_translation_carries_mask_and_denoise() {
        let provider = ComfyUiProvider;
        let req = sample_inpaint_request();
        let payload = provider.translate_inpaint(&req);
        let workflow = payload["prompt"].as_object().expect("workflow obj");
        assert!(workflow.get("9").is_some(), "mask loader node present");
        assert!(workflow.get("10").is_some(), "VAE encode for inpaint node");
        assert_eq!(workflow["3"]["inputs"]["denoise"], 0.32);
        assert_eq!(workflow["10"]["inputs"]["grow_mask_by"], 12);
    }

    #[test]
    fn comfyui_parses_history_outputs_into_canonical_response() {
        let provider = ComfyUiProvider;
        let raw = json!({
            "prompt_id": "abc",
            "outputs": {
                "9": {
                    "images": [
                        {"filename": "iqh_0001.png", "subfolder": "iqh", "type": "output", "seed": 42},
                        {"filename": "iqh_0002.png", "subfolder": "", "type": "temp"}
                    ]
                }
            }
        });
        let parsed = provider.parse_response(&raw).expect("parse ok");
        assert_eq!(parsed.images.len(), 2);
        assert_eq!(parsed.images[0].uri, "comfyui://output/iqh/iqh_0001.png");
        assert_eq!(parsed.images[0].seed, 42);
        assert_eq!(parsed.images[1].uri, "comfyui://temp/iqh_0002.png");
        assert_eq!(parsed.metadata["prompt_id"], "abc");
    }

    #[test]
    fn comfyui_parse_rejects_empty_or_missing_outputs() {
        let provider = ComfyUiProvider;
        let err = provider
            .parse_response(&json!({"foo": "bar"}))
            .expect_err("missing outputs");
        assert!(err.contains("missing `outputs`"));
        let err = provider
            .parse_response(&json!({"outputs": {"3": {"images": []}}}))
            .expect_err("no images");
        assert!(err.contains("no images"));
    }

    #[test]
    fn diffusers_translation_uses_canonical_keys() {
        let provider = DiffusersProvider;
        let req = sample_generate_request();
        let payload = provider.translate_generate(&req);
        assert_eq!(payload["num_inference_steps"], 30);
        assert_eq!(payload["guidance_scale"], 6.5);
        assert_eq!(payload["scheduler"], "dpmpp_2m");
        assert_eq!(payload["style_preset"], "cinematic");
        assert_eq!(payload["num_images_per_prompt"], 2);
        assert_eq!(payload["seed"], 42);
    }

    #[test]
    fn diffusers_parse_handles_artifacts_and_images_variants() {
        let provider = DiffusersProvider;
        let artifacts = json!({"artifacts": [{"uri": "s3://a.png", "seed": 7}]});
        let parsed = provider.parse_response(&artifacts).expect("artifacts");
        assert_eq!(parsed.images[0].uri, "s3://a.png");
        assert_eq!(parsed.images[0].seed, 7);

        let images = json!({"images": [{"url": "https://x/y.png"}]});
        let parsed = provider.parse_response(&images).expect("images");
        assert_eq!(parsed.images[0].uri, "https://x/y.png");
    }

    #[test]
    fn diffusers_endpoint_routes_generate_and_inpaint() {
        let provider = DiffusersProvider;
        assert_eq!(
            provider.endpoint(ImageOp::Generate, "http://h:8000/"),
            "http://h:8000/v1/text-to-image"
        );
        assert_eq!(
            provider.endpoint(ImageOp::Inpaint, "http://h:8000"),
            "http://h:8000/v1/inpaint"
        );
    }

    #[test]
    fn internal_worker_envelopes_payload_in_task_object() {
        let provider = InternalWorkerProvider;
        let req = sample_generate_request();
        let payload = provider.translate_generate(&req);
        assert_eq!(payload["task"], "generate_image");
        assert_eq!(payload["params"]["seed"], 42);
        assert_eq!(payload["submitter"], "rusty-harness");
    }

    #[test]
    fn internal_worker_parses_result_block_with_seeds_and_uris() {
        let provider = InternalWorkerProvider;
        let raw = json!({
            "job_id": "job-99",
            "queue": "highprio",
            "duration_ms": 4200,
            "result": {
                "images": [
                    {"uri": "wkr://job-99/0.png", "seed": 1},
                    {"path": "wkr://job-99/1.png"}
                ]
            }
        });
        let parsed = provider.parse_response(&raw).expect("parse worker resp");
        assert_eq!(parsed.images.len(), 2);
        assert_eq!(parsed.images[0].seed, 1);
        assert_eq!(parsed.metadata["job_id"], "job-99");
        assert_eq!(parsed.metadata["queue"], "highprio");
    }

    #[test]
    fn registry_routes_unknown_provider_to_explicit_error() {
        let registry = ImageProviderRegistry::builtin();
        assert_eq!(registry.names().len(), 3);
        let err = registry
            .translate_generate("nope", "http://x", &sample_generate_request())
            .expect_err("unknown provider");
        assert!(err.contains("unknown image provider"));
        assert!(err.contains("comfyui"));
        assert!(err.contains("diffusers"));
        assert!(err.contains("internal_worker"));
    }

    #[test]
    fn registry_translate_generate_resolves_endpoint_per_provider() {
        let registry = ImageProviderRegistry::builtin();
        let comfy = registry
            .translate_generate(
                "comfyui",
                "http://localhost:8188",
                &sample_generate_request(),
            )
            .expect("comfy translate");
        assert_eq!(comfy.endpoint, "http://localhost:8188/prompt");
        let diffusers = registry
            .translate_generate("diffusers", "http://h:9000", &sample_generate_request())
            .expect("diffusers translate");
        assert_eq!(diffusers.endpoint, "http://h:9000/v1/text-to-image");
        let worker = registry
            .translate_generate("internal_worker", "http://q", &sample_generate_request())
            .expect("worker translate");
        assert_eq!(worker.endpoint, "http://q/jobs/generate");
    }

    #[test]
    fn registry_parse_response_dispatches_per_provider() {
        let registry = ImageProviderRegistry::builtin();
        let comfy = registry
            .parse_response(
                "comfyui",
                &json!({
                    "outputs": {"9": {"images": [{"filename": "a.png", "subfolder": "", "type": "output"}]}}
                }),
            )
            .expect("comfy parse");
        assert_eq!(comfy.images[0].uri, "comfyui://output/a.png");

        let diffusers = registry
            .parse_response(
                "diffusers",
                &json!({"artifacts": [{"uri": "s3://a", "seed": 1}]}),
            )
            .expect("diffusers parse");
        assert_eq!(diffusers.images[0].seed, 1);
    }
}
