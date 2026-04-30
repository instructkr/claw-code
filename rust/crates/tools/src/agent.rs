//! Agent spawning and lifecycle management for multi-agent workflows.
//!
//! This module provides the core infrastructure for spawning and managing
//! sub-agents that work in parallel on tasks. Key features:
//!
//! - **Agent creation**: Spawn agents with specific roles and prompts
//! - **Manifest management**: Track agent state and progress
//! - **Subagent types**: Specialized agent roles (Explore, Plan, Verification, etc.)
//!
//! ## Multi-Agent Architecture
//!
//! Agents are spawned as separate threads with their own context. They can:
//! - Claim tasks to prevent duplicate work
//! - Report progress to team inbox
//! - Be terminated via kill signals

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// --- Input Types ---

#[derive(Debug, Deserialize)]
pub struct AgentInput {
    pub description: String,
    pub prompt: String,
    pub subagent_type: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    pub task_id: Option<String>,
}

// --- Output Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "subagentType")]
    pub subagent_type: Option<String>,
    pub model: Option<String>,
    pub status: String,
    #[serde(rename = "outputFile")]
    pub output_file: String,
    #[serde(rename = "manifestFile")]
    pub manifest_file: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(rename = "laneEvents", default, skip_serializing_if = "Vec::is_empty")]
    pub lane_events: Vec<runtime::LaneEvent>,
    #[serde(rename = "currentBlocker", skip_serializing_if = "Option::is_none")]
    pub current_blocker: Option<runtime::LaneEventBlocker>,
    #[serde(rename = "derivedState")]
    pub derived_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "teamId", skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(rename = "taskId", skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

// --- Directory Management ---

/// Get the agent store directory for manifests and outputs.
pub fn agent_store_dir() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CLAWD_AGENT_STORE") {
        return Ok(PathBuf::from(path));
    }
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if let Some(workspace_root) = cwd.ancestors().nth(2) {
        return Ok(workspace_root.join(".clawd-agents"));
    }
    Ok(cwd.join(".clawd-agents"))
}

// --- Agent ID Generation ---

/// Generate a unique agent ID.
pub fn make_agent_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("agent-{nanos}")
}

/// Convert a description into a URL-safe slug for agent names.
pub fn slugify_agent_name(description: &str) -> String {
    let mut out = description
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').chars().take(32).collect()
}

// --- Subagent Type Normalization ---

/// Normalize subagent type to canonical form.
pub fn normalize_subagent_type(subagent_type: Option<&str>) -> String {
    let trimmed = subagent_type.map(str::trim).unwrap_or_default();
    if trimmed.is_empty() {
        return String::from("general-purpose");
    }

    match canonical_tool_token(trimmed).as_str() {
        "general" | "generalpurpose" | "generalpurposeagent" => String::from("general-purpose"),
        "explore" | "explorer" | "exploreagent" => String::from("Explore"),
        "plan" | "planagent" => String::from("Plan"),
        "verification" | "verificationagent" | "verify" | "verifier" => {
            String::from("Verification")
        }
        "reviewer" | "review" | "reviewagent" => String::from("Reviewer"),
        "clawguide" | "clawguideagent" | "guide" => String::from("claw-guide"),
        "statusline" | "statuslinesetup" => String::from("statusline-setup"),
        _ => trimmed.to_string(),
    }
}

/// Normalize a tool token to canonical lowercase form.
pub fn canonical_tool_token(value: &str) -> String {
    let stripped = value.trim().trim_start_matches('/').to_lowercase();
    let mut canonical = String::new();
    for ch in stripped.chars() {
        if ch.is_ascii_alphanumeric() {
            canonical.push(ch);
        }
    }
    if canonical.is_empty() {
        canonical = stripped;
    }
    canonical
}

// --- Timestamp Helpers ---

/// Get the current time as ISO 8601 string.
pub fn iso8601_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
