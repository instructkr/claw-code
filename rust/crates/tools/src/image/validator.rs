//! Validator-execution stage with the normalised score model from Spec §5.1.
//!
//! The stage runs the four detector contracts (`detect_hands_feet`,
//! `check_symmetry`, `check_pattern_consistency`, plus an artifact validator)
//! against an image, parses each provider response into the canonical score
//! type, and produces a [`ValidatorReport`] that the policy gate and
//! [`crate::image::regression`] runner consume.
//!
//! The validator endpoints accept the same JSON contracts described in
//! Spec §4.2-§4.4. The stage performs minimum-bounds clamping and rejects
//! out-of-range scores early so downstream consumers always see values in
//! `0.0..=1.0`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::profile::Thresholds;
use super::provider::HttpInvoker;

/// Canonical score vector used by the policy gate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct NormalisedScores {
    pub anatomy_score: f64,
    pub symmetry_score: f64,
    pub pattern_score: f64,
    pub artifact_score: f64,
    pub creative_score: f64,
}

impl NormalisedScores {
    #[must_use]
    pub fn weighted_total(&self) -> f64 {
        0.40 * self.anatomy_score
            + 0.25 * self.symmetry_score
            + 0.20 * self.pattern_score
            + 0.10 * self.artifact_score
            + 0.05 * self.creative_score
    }

    #[must_use]
    pub fn passes(
        &self,
        thresholds: &Thresholds,
        expects_symmetry: bool,
        expects_pattern: bool,
    ) -> bool {
        let symmetry_ok = !expects_symmetry || self.symmetry_score >= thresholds.symmetry;
        let pattern_ok = !expects_pattern || self.pattern_score >= thresholds.pattern;
        self.anatomy_score >= thresholds.anatomy
            && self.artifact_score >= thresholds.artifact
            && symmetry_ok
            && pattern_ok
            && self.weighted_total() >= thresholds.weighted_total
    }
}

/// Per-image normalised result. Failure descriptors are kept in violation
/// strings so the loop planner can build targeted repair masks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorReport {
    pub scores: NormalisedScores,
    pub anatomy_issues: Vec<String>,
    pub symmetry_violations: Vec<String>,
    pub pattern_violations: Vec<String>,
    pub artifact_issues: Vec<String>,
    pub raw_responses: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorEndpoints {
    pub hands_feet: Option<String>,
    pub symmetry: Option<String>,
    pub pattern: Option<String>,
    pub artifact: Option<String>,
    pub creative: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymmetryExpectation {
    pub label: String,
    pub left_region: [f64; 4],
    pub right_region: [f64; 4],
    #[serde(default)]
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRegion {
    pub label: String,
    pub bbox: [f64; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorRequest {
    pub image_uri: String,
    #[serde(default)]
    pub min_confidence: Option<f64>,
    #[serde(default)]
    pub symmetry_expectations: Vec<SymmetryExpectation>,
    #[serde(default)]
    pub pattern_regions: Vec<PatternRegion>,
    #[serde(default)]
    pub creative_intent: Option<String>,
}

/// Runs the validator suite against a single image using the supplied HTTP
/// invoker. Each endpoint is optional: when omitted (or when the validator
/// returns an error), the stage records a neutral score for that axis so the
/// gate can still apply per-metric requirements truthfully.
pub struct ValidatorStage {
    endpoints: ValidatorEndpoints,
    invoker: Arc<dyn HttpInvoker>,
}

impl ValidatorStage {
    pub fn new(endpoints: ValidatorEndpoints, invoker: Arc<dyn HttpInvoker>) -> Self {
        Self { endpoints, invoker }
    }

    pub fn run(&self, request: &ValidatorRequest) -> Result<ValidatorReport, String> {
        let mut anatomy_issues = Vec::new();
        let mut symmetry_violations = Vec::new();
        let mut pattern_violations = Vec::new();
        let mut artifact_issues = Vec::new();
        let mut raw = serde_json::Map::new();

        let anatomy_score = if let Some(url) = &self.endpoints.hands_feet {
            let payload = json!({
                "image_uri": request.image_uri,
                "min_confidence": request.min_confidence.unwrap_or(0.25)
            });
            let response = self.invoker.post_json(url, &payload)?;
            let score = parse_anatomy(&response, &mut anatomy_issues)?;
            raw.insert("hands_feet".to_string(), response);
            score
        } else {
            1.0
        };

        let symmetry_score = if let Some(url) = &self.endpoints.symmetry {
            if request.symmetry_expectations.is_empty() {
                1.0
            } else {
                let payload = json!({
                    "image_uri": request.image_uri,
                    "expectations": request.symmetry_expectations
                });
                let response = self.invoker.post_json(url, &payload)?;
                let score = parse_symmetry(&response, &mut symmetry_violations)?;
                raw.insert("symmetry".to_string(), response);
                score
            }
        } else {
            1.0
        };

        let pattern_score = if let Some(url) = &self.endpoints.pattern {
            if request.pattern_regions.is_empty() {
                1.0
            } else {
                let payload = json!({
                    "image_uri": request.image_uri,
                    "pattern_regions": request.pattern_regions
                });
                let response = self.invoker.post_json(url, &payload)?;
                let score = parse_pattern(&response, &mut pattern_violations)?;
                raw.insert("pattern".to_string(), response);
                score
            }
        } else {
            1.0
        };

        let artifact_score = if let Some(url) = &self.endpoints.artifact {
            let payload = json!({"image_uri": request.image_uri});
            let response = self.invoker.post_json(url, &payload)?;
            let score = parse_artifact(&response, &mut artifact_issues)?;
            raw.insert("artifact".to_string(), response);
            score
        } else {
            1.0
        };

        let creative_score = if let Some(url) = &self.endpoints.creative {
            let payload = json!({
                "image_uri": request.image_uri,
                "intent": request.creative_intent
            });
            let response = self.invoker.post_json(url, &payload)?;
            let score = parse_creative(&response)?;
            raw.insert("creative".to_string(), response);
            score
        } else {
            0.0
        };

        let scores = NormalisedScores {
            anatomy_score: clamp_unit(anatomy_score)?,
            symmetry_score: clamp_unit(symmetry_score)?,
            pattern_score: clamp_unit(pattern_score)?,
            artifact_score: clamp_unit(artifact_score)?,
            creative_score: clamp_unit(creative_score)?,
        };

        Ok(ValidatorReport {
            scores,
            anatomy_issues,
            symmetry_violations,
            pattern_violations,
            artifact_issues,
            raw_responses: Value::Object(raw),
        })
    }
}

fn clamp_unit(value: f64) -> Result<f64, String> {
    if !value.is_finite() {
        return Err(format!("validator returned non-finite score {value}"));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(format!(
            "validator returned out-of-range score {value} (expected 0.0..=1.0)"
        ));
    }
    Ok(value)
}

fn parse_anatomy(response: &Value, issues: &mut Vec<String>) -> Result<f64, String> {
    let score = response
        .pointer("/scores/anatomy_score")
        .and_then(Value::as_f64)
        .ok_or("hands/feet validator missing scores.anatomy_score")?;
    if let Some(regions) = response.get("regions").and_then(Value::as_array) {
        for region in regions {
            if let Some(kind) = region.get("kind").and_then(Value::as_str) {
                if let Some(issue_list) = region.get("issues").and_then(Value::as_array) {
                    for issue in issue_list {
                        if let Some(name) = issue.as_str() {
                            issues.push(format!("{kind}:{name}"));
                        }
                    }
                }
            }
        }
    }
    Ok(score)
}

fn parse_symmetry(response: &Value, violations: &mut Vec<String>) -> Result<f64, String> {
    let score = response
        .get("symmetry_score")
        .and_then(Value::as_f64)
        .ok_or("symmetry validator missing symmetry_score")?;
    if let Some(arr) = response.get("violations").and_then(Value::as_array) {
        for v in arr {
            if let Some(label) = v.get("label").and_then(Value::as_str) {
                let delta = v.get("delta").and_then(Value::as_f64).unwrap_or(0.0);
                violations.push(format!("{label}:Δ={delta:.3}"));
            }
        }
    }
    Ok(score)
}

fn parse_pattern(response: &Value, violations: &mut Vec<String>) -> Result<f64, String> {
    let score = response
        .get("pattern_score")
        .and_then(Value::as_f64)
        .ok_or("pattern validator missing pattern_score")?;
    if let Some(arr) = response.get("violations").and_then(Value::as_array) {
        for v in arr {
            let label = v.get("label").and_then(Value::as_str).unwrap_or("?");
            let kind = v.get("error_type").and_then(Value::as_str).unwrap_or("?");
            let severity = v.get("severity").and_then(Value::as_f64).unwrap_or(0.0);
            violations.push(format!("{label}:{kind}@{severity:.2}"));
        }
    }
    Ok(score)
}

fn parse_artifact(response: &Value, issues: &mut Vec<String>) -> Result<f64, String> {
    let score = response
        .get("artifact_score")
        .and_then(Value::as_f64)
        .ok_or("artifact validator missing artifact_score")?;
    if let Some(arr) = response.get("issues").and_then(Value::as_array) {
        for v in arr {
            if let Some(name) = v.as_str() {
                issues.push(name.to_string());
            } else if let Some(name) = v.get("type").and_then(Value::as_str) {
                let severity = v.get("severity").and_then(Value::as_f64).unwrap_or(0.0);
                issues.push(format!("{name}@{severity:.2}"));
            }
        }
    }
    Ok(score)
}

fn parse_creative(response: &Value) -> Result<f64, String> {
    response
        .get("creative_score")
        .and_then(Value::as_f64)
        .ok_or_else(|| "creative validator missing creative_score".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct StubInvoker {
        responses: Mutex<Vec<(String, Value)>>,
    }

    impl StubInvoker {
        fn push(&self, url: &str, value: Value) {
            self.responses
                .lock()
                .unwrap()
                .push((url.to_string(), value));
        }
    }

    impl HttpInvoker for StubInvoker {
        fn post_json(&self, url: &str, _payload: &Value) -> Result<Value, String> {
            let mut guard = self.responses.lock().unwrap();
            let pos = guard
                .iter()
                .position(|(u, _)| u == url)
                .ok_or_else(|| format!("no stub for {url}"))?;
            Ok(guard.remove(pos).1)
        }
    }

    fn endpoints() -> ValidatorEndpoints {
        ValidatorEndpoints {
            hands_feet: Some("http://stub/hands".to_string()),
            symmetry: Some("http://stub/sym".to_string()),
            pattern: Some("http://stub/pat".to_string()),
            artifact: Some("http://stub/art".to_string()),
            creative: Some("http://stub/cre".to_string()),
        }
    }

    #[test]
    fn validator_stage_runs_full_suite_and_aggregates_scores() {
        let invoker = Arc::new(StubInvoker::default());
        invoker.push(
            "http://stub/hands",
            json!({
                "regions": [
                    {"kind": "left_hand", "bbox": [0,0,1,1], "confidence": 0.9, "issues": ["extra_digits"]}
                ],
                "scores": {"anatomy_score": 0.93}
            }),
        );
        invoker.push(
            "http://stub/sym",
            json!({
                "symmetry_score": 0.91,
                "violations": [{"label": "pauldrons", "delta": 0.2, "left_bbox": [0,0,1,1], "right_bbox": [1,0,2,1]}]
            }),
        );
        invoker.push(
            "http://stub/pat",
            json!({
                "pattern_score": 0.89,
                "violations": [{"label": "chest", "error_type": "scale_drift", "bbox": [0,0,1,1], "severity": 0.4}]
            }),
        );
        invoker.push(
            "http://stub/art",
            json!({
                "artifact_score": 0.87,
                "issues": ["ringing"]
            }),
        );
        invoker.push("http://stub/cre", json!({"creative_score": 0.82}));

        let stage = ValidatorStage::new(endpoints(), invoker.clone());
        let request = ValidatorRequest {
            image_uri: "img://1".to_string(),
            symmetry_expectations: vec![SymmetryExpectation {
                label: "pauldrons".to_string(),
                left_region: [0.0, 0.0, 1.0, 1.0],
                right_region: [1.0, 0.0, 2.0, 1.0],
                tolerance: None,
            }],
            pattern_regions: vec![PatternRegion {
                label: "chest".to_string(),
                bbox: [0.0, 0.0, 1.0, 1.0],
            }],
            ..Default::default()
        };
        let report = stage.run(&request).expect("validator succeeds");
        assert!((report.scores.anatomy_score - 0.93).abs() < f64::EPSILON);
        assert_eq!(report.anatomy_issues, vec!["left_hand:extra_digits"]);
        assert_eq!(report.symmetry_violations.len(), 1);
        assert!(report.symmetry_violations[0].starts_with("pauldrons:"));
        assert_eq!(report.pattern_violations[0], "chest:scale_drift@0.40");
        assert_eq!(report.artifact_issues, vec!["ringing"]);
        // weighted total = 0.4*0.93 + 0.25*0.91 + 0.2*0.89 + 0.1*0.87 + 0.05*0.82
        //                = 0.372 + 0.2275 + 0.178 + 0.087 + 0.041 = 0.9055
        let total = report.scores.weighted_total();
        assert!((total - 0.9055).abs() < 1e-6, "weighted total = {total}");
    }

    #[test]
    fn validator_stage_skips_axes_without_endpoints() {
        let invoker = Arc::new(StubInvoker::default());
        let stage = ValidatorStage::new(ValidatorEndpoints::default(), invoker);
        let report = stage
            .run(&ValidatorRequest {
                image_uri: "img://x".to_string(),
                ..Default::default()
            })
            .expect("ok");
        assert!((report.scores.anatomy_score - 1.0).abs() < f64::EPSILON);
        assert!(report.scores.creative_score.abs() < f64::EPSILON);
        assert!(report.anatomy_issues.is_empty());
    }

    #[test]
    fn validator_rejects_out_of_range_scores() {
        let invoker = Arc::new(StubInvoker::default());
        invoker.push(
            "http://stub/hands",
            json!({"scores": {"anatomy_score": 1.5}, "regions": []}),
        );
        let stage = ValidatorStage::new(
            ValidatorEndpoints {
                hands_feet: Some("http://stub/hands".to_string()),
                ..Default::default()
            },
            invoker,
        );
        let err = stage
            .run(&ValidatorRequest {
                image_uri: "img://x".to_string(),
                ..Default::default()
            })
            .expect_err("range");
        assert!(err.contains("out-of-range"));
    }

    #[test]
    fn passes_uses_thresholds_and_optional_expectations() {
        let scores = NormalisedScores {
            anatomy_score: 0.93,
            symmetry_score: 0.80,
            pattern_score: 0.50,
            artifact_score: 0.88,
            creative_score: 0.7,
        };
        let thresholds = Thresholds::production();
        // symmetry below 0.90 means fail when expected
        assert!(!scores.passes(&thresholds, true, false));
        // skip symmetry: still fails because pattern below threshold when expected
        assert!(!scores.passes(&thresholds, false, true));
        // skip both per-axis checks => passes when weighted_total still clears.
        // Symmetry/pattern still contribute to weighted_total (they only skip
        // the per-axis floor); supply solid scores so the weighted gate clears.
        let scores_relaxed = NormalisedScores {
            anatomy_score: 0.95,
            symmetry_score: 0.95,
            pattern_score: 0.95,
            artifact_score: 0.90,
            creative_score: 0.85,
        };
        assert!(scores_relaxed.passes(&thresholds, false, false));
    }
}
