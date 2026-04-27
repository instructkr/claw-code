//! Operating-profile registry from Spec §7.
//!
//! The gate alone only exposes per-axis thresholds. The harness also needs a
//! profile abstraction that picks generation hyperparameters (CFG/step
//! ranges, seed-sweep size), retry/iteration budgets, and the threshold
//! bundle to apply. This module owns the canonical Exploration / Production /
//! Strict profiles and supports user-defined extensions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Thresholds {
    pub anatomy: f64,
    pub symmetry: f64,
    pub pattern: f64,
    pub artifact: f64,
    pub creative: f64,
    pub weighted_total: f64,
}

impl Thresholds {
    /// Strict thresholds from §5.2.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            anatomy: 0.92,
            symmetry: 0.90,
            pattern: 0.88,
            artifact: 0.85,
            creative: 0.0,
            weighted_total: 0.90,
        }
    }

    /// Exploration thresholds — lower bar to discover creative compositions
    /// (§7.1).
    #[must_use]
    pub const fn exploration() -> Self {
        Self {
            anatomy: 0.85,
            symmetry: 0.82,
            pattern: 0.80,
            artifact: 0.78,
            creative: 0.0,
            weighted_total: 0.82,
        }
    }

    /// Strictest configuration used by release gates and CI regression runs.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            anatomy: 0.95,
            symmetry: 0.93,
            pattern: 0.92,
            artifact: 0.90,
            creative: 0.0,
            weighted_total: 0.93,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationHyperparameters {
    pub steps_min: u32,
    pub steps_max: u32,
    pub cfg_min: f64,
    pub cfg_max: f64,
    pub seed_count: u32,
    pub max_iterations: u32,
    pub default_sampler: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileSpec {
    pub name: String,
    pub description: String,
    pub thresholds: Thresholds,
    pub hyperparameters: GenerationHyperparameters,
    /// Whether the profile expects symmetry validators to be present when the
    /// scene declares symmetry labels.
    pub requires_symmetry: bool,
    pub requires_pattern: bool,
}

impl ProfileSpec {
    #[must_use]
    pub fn exploration() -> Self {
        Self {
            name: "exploration".to_string(),
            description: "Discovery profile — wider CFG/steps range and 24 seeds with relaxed gate"
                .to_string(),
            thresholds: Thresholds::exploration(),
            hyperparameters: GenerationHyperparameters {
                steps_min: 24,
                steps_max: 48,
                cfg_min: 4.5,
                cfg_max: 8.5,
                seed_count: 24,
                max_iterations: 2,
                default_sampler: "dpmpp_2m_karras".to_string(),
            },
            requires_symmetry: false,
            requires_pattern: false,
        }
    }

    #[must_use]
    pub fn production() -> Self {
        Self {
            name: "production".to_string(),
            description: "Tight style lock with up to 4 correction cycles and §5.2 thresholds"
                .to_string(),
            thresholds: Thresholds::production(),
            hyperparameters: GenerationHyperparameters {
                steps_min: 28,
                steps_max: 40,
                cfg_min: 5.0,
                cfg_max: 7.5,
                seed_count: 8,
                max_iterations: 4,
                default_sampler: "dpmpp_2m_karras".to_string(),
            },
            requires_symmetry: true,
            requires_pattern: true,
        }
    }

    #[must_use]
    pub fn strict() -> Self {
        Self {
            name: "strict".to_string(),
            description: "Release-gate profile — strictest thresholds, 12 seeds, 2 iterations max"
                .to_string(),
            thresholds: Thresholds::strict(),
            hyperparameters: GenerationHyperparameters {
                steps_min: 32,
                steps_max: 40,
                cfg_min: 5.5,
                cfg_max: 7.0,
                seed_count: 12,
                max_iterations: 2,
                default_sampler: "dpmpp_2m_karras".to_string(),
            },
            requires_symmetry: true,
            requires_pattern: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileRegistry {
    profiles: BTreeMap<String, ProfileSpec>,
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ProfileRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self {
            profiles: BTreeMap::new(),
        };
        registry.insert(ProfileSpec::exploration());
        registry.insert(ProfileSpec::production());
        registry.insert(ProfileSpec::strict());
        registry
    }

    pub fn insert(&mut self, profile: ProfileSpec) {
        self.profiles.insert(profile.name.clone(), profile);
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Result<&ProfileSpec, String> {
        self.profiles.get(name).ok_or_else(|| {
            let known = self.profiles.keys().cloned().collect::<Vec<_>>().join(", ");
            format!("unknown image profile `{name}` (known: {known})")
        })
    }

    /// Validate generation parameters against the profile and return the
    /// concrete sampler / step / CFG values to use. Out-of-range parameters
    /// are clamped and reported in `warnings`.
    pub fn resolve(&self, name: &str, params: ResolveParams) -> Result<ResolvedParams, String> {
        let profile = self.get(name)?;
        let mut warnings = Vec::new();

        let steps = match params.requested_steps {
            Some(s) => clamp_to_range(
                s,
                profile.hyperparameters.steps_min,
                profile.hyperparameters.steps_max,
                "steps",
                &mut warnings,
            ),
            None => profile.hyperparameters.steps_max,
        };
        let cfg = match params.requested_cfg {
            Some(c) => clamp_to_range_f(
                c,
                profile.hyperparameters.cfg_min,
                profile.hyperparameters.cfg_max,
                "cfg",
                &mut warnings,
            ),
            None => f64::midpoint(
                profile.hyperparameters.cfg_min,
                profile.hyperparameters.cfg_max,
            ),
        };
        let sampler = params
            .requested_sampler
            .unwrap_or_else(|| profile.hyperparameters.default_sampler.clone());
        let seeds = params.requested_seeds.unwrap_or_else(|| {
            (0..profile.hyperparameters.seed_count)
                .map(u64::from)
                .collect()
        });

        let budget_4x = profile.hyperparameters.seed_count.saturating_mul(4) as usize;
        if seeds.len() > budget_4x {
            warnings.push(format!(
                "seed sweep ({}) exceeds 4× profile budget ({}); consider profile=exploration",
                seeds.len(),
                profile.hyperparameters.seed_count
            ));
        }

        Ok(ResolvedParams {
            profile_name: profile.name.clone(),
            thresholds: profile.thresholds,
            steps,
            cfg,
            sampler,
            seeds,
            max_iterations: profile.hyperparameters.max_iterations,
            requires_symmetry: profile.requires_symmetry,
            requires_pattern: profile.requires_pattern,
            warnings,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolveParams {
    pub requested_steps: Option<u32>,
    pub requested_cfg: Option<f64>,
    pub requested_sampler: Option<String>,
    pub requested_seeds: Option<Vec<u64>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedParams {
    pub profile_name: String,
    pub thresholds: Thresholds,
    pub steps: u32,
    pub cfg: f64,
    pub sampler: String,
    pub seeds: Vec<u64>,
    pub max_iterations: u32,
    pub requires_symmetry: bool,
    pub requires_pattern: bool,
    pub warnings: Vec<String>,
}

fn clamp_to_range(value: u32, min: u32, max: u32, label: &str, warnings: &mut Vec<String>) -> u32 {
    if value < min {
        warnings.push(format!("{label}={value} below profile min {min}; clamped"));
        return min;
    }
    if value > max {
        warnings.push(format!("{label}={value} above profile max {max}; clamped"));
        return max;
    }
    value
}

fn clamp_to_range_f(
    value: f64,
    min: f64,
    max: f64,
    label: &str,
    warnings: &mut Vec<String>,
) -> f64 {
    if value < min {
        warnings.push(format!(
            "{label}={value:.3} below profile min {min:.3}; clamped"
        ));
        return min;
    }
    if value > max {
        warnings.push(format!(
            "{label}={value:.3} above profile max {max:.3}; clamped"
        ));
        return max;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_three_builtin_profiles() {
        let registry = ProfileRegistry::builtin();
        let mut names = registry.names();
        names.sort();
        assert_eq!(names, vec!["exploration", "production", "strict"]);
    }

    #[test]
    fn unknown_profile_lookup_lists_known_choices() {
        let registry = ProfileRegistry::builtin();
        let err = registry.get("nope").expect_err("unknown");
        assert!(err.contains("exploration"));
        assert!(err.contains("production"));
        assert!(err.contains("strict"));
    }

    #[test]
    fn resolve_clamps_out_of_range_steps_and_cfg() {
        let registry = ProfileRegistry::builtin();
        let resolved = registry
            .resolve(
                "production",
                ResolveParams {
                    requested_steps: Some(2),
                    requested_cfg: Some(15.0),
                    ..Default::default()
                },
            )
            .expect("resolve");
        assert_eq!(resolved.steps, 28);
        assert!((resolved.cfg - 7.5).abs() < f64::EPSILON);
        assert_eq!(resolved.warnings.len(), 2);
        assert!(resolved.warnings[0].contains("below profile min"));
        assert!(resolved.warnings[1].contains("above profile max"));
    }

    #[test]
    fn resolve_uses_profile_defaults_when_unset() {
        let registry = ProfileRegistry::builtin();
        let resolved = registry
            .resolve("exploration", ResolveParams::default())
            .expect("ok");
        assert_eq!(resolved.steps, 48); // exploration max
        assert!((resolved.cfg - 6.5).abs() < 1e-6); // midpoint of 4.5..=8.5
        assert_eq!(resolved.seeds.len(), 24);
        assert_eq!(resolved.max_iterations, 2);
    }

    #[test]
    fn resolve_returns_warning_when_seed_sweep_far_exceeds_budget() {
        let registry = ProfileRegistry::builtin();
        let resolved = registry
            .resolve(
                "strict",
                ResolveParams {
                    requested_seeds: Some((0..200).collect()),
                    ..Default::default()
                },
            )
            .expect("ok");
        assert!(resolved
            .warnings
            .iter()
            .any(|w| w.contains("exceeds 4× profile budget")));
    }

    #[test]
    fn user_defined_profile_can_be_inserted_and_resolved() {
        let mut registry = ProfileRegistry::builtin();
        registry.insert(ProfileSpec {
            name: "custom-fast".to_string(),
            description: "Fast preview profile".to_string(),
            thresholds: Thresholds::exploration(),
            hyperparameters: GenerationHyperparameters {
                steps_min: 8,
                steps_max: 16,
                cfg_min: 4.0,
                cfg_max: 5.5,
                seed_count: 4,
                max_iterations: 1,
                default_sampler: "euler".to_string(),
            },
            requires_symmetry: false,
            requires_pattern: false,
        });
        let resolved = registry
            .resolve("custom-fast", ResolveParams::default())
            .expect("custom resolve");
        assert_eq!(resolved.profile_name, "custom-fast");
        assert_eq!(resolved.sampler, "euler");
        assert_eq!(resolved.seeds.len(), 4);
        assert_eq!(resolved.max_iterations, 1);
    }

    #[test]
    fn thresholds_strict_is_tighter_than_production_is_tighter_than_exploration() {
        let strict = Thresholds::strict();
        let prod = Thresholds::production();
        let expl = Thresholds::exploration();
        assert!(strict.anatomy > prod.anatomy);
        assert!(prod.anatomy > expl.anatomy);
        assert!(strict.weighted_total > prod.weighted_total);
        assert!(prod.weighted_total > expl.weighted_total);
    }
}
