//! Image-quality harness implementation that backs Spec §10.
//!
//! The submodules cover the four pieces of the plan that are not exposed by
//! the bare HTTP forwarders in `lib.rs`:
//!
//! * [`provider`] — adapters that translate the canonical generation/inpaint
//!   payload into the request format expected by `ComfyUI`, a `Diffusers` REST
//!   service, or an internal image-render worker, and parse their responses
//!   back into the canonical schema.
//! * [`validator`] — a stage that runs the detector/symmetry/pattern/artifact
//!   validators against an image and returns a normalised score vector that
//!   feeds the policy gate.
//! * [`profile`] — operating-profile registry (Exploration, Production,
//!   Strict, plus user-defined entries) including thresholds, seed-sweep
//!   counts, and CFG/step ranges.
//! * [`regression`] — fixture-driven runner that exercises a scene pack
//!   through the generator/validator/gate loop and emits the JSON +
//!   markdown CI summary mandated by §8.

pub mod profile;
pub mod provider;
pub mod regression;
pub mod validator;

pub use profile::{ProfileRegistry, ProfileSpec, Thresholds};
pub use provider::{
    CanonicalControlInput, CanonicalGenerationRequest, CanonicalGenerationResponse,
    CanonicalInpaintRequest, GeneratedImage, HttpInvoker, ImageProvider, ImageProviderRegistry,
    ProviderInvocation,
};
pub use regression::{
    RegressionFixture, RegressionRun, RegressionRunReport, RegressionSummary, SceneOutcome,
};
pub use validator::{NormalisedScores, ValidatorReport, ValidatorStage};
