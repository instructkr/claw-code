use std::collections::BTreeMap;

use runtime::{ToolError, ToolExecutor};

/// A registry of tool names and their descriptions for use with the SDK.
///
/// This wraps the concept of the existing `GlobalToolRegistry` but provides
/// a simpler, SDK-friendly interface.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tool_names: Vec<String>,
    descriptions: BTreeMap<String, String>,
}

impl ToolRegistry {
    /// Create a new empty tool registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_names: Vec::new(),
            descriptions: BTreeMap::new(),
        }
    }

    /// Register a built-in tool by name.
    pub fn register_builtin(&mut self, name: &str) {
        if !self.tool_names.contains(&name.to_string()) {
            self.tool_names.push(name.to_string());
            self.descriptions
                .insert(name.to_string(), format!("built-in tool: {name}"));
        }
    }

    /// Check if a tool is registered.
    #[must_use]
    pub fn has_tool(&self, name: &str) -> bool {
        self.tool_names.iter().any(|n| n == name)
    }

    /// Get all registered tool names.
    #[must_use]
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }

    /// Get the description for a tool.
    #[must_use]
    pub fn description(&self, name: &str) -> Option<&str> {
        self.descriptions.get(name).map(String::as_str)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a default set of built-in tools for SDK usage.
#[must_use]
pub fn create_builtin_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for name in &[
        "read_file",
        "write_file",
        "edit_file",
        "glob_search",
        "grep_search",
        "bash",
        "WebFetch",
        "WebSearch",
        "TodoWrite",
        "Agent",
    ] {
        registry.register_builtin(name);
    }
    registry
}

/// A simple tool executor that delegates to the actual execution implementation.
/// This is used by the SDK when a full `GlobalToolRegistry` isn't available.
#[derive(Debug, Clone)]
pub struct SdkToolExecutor {
    tool_specs: BTreeMap<String, String>,
}

impl SdkToolExecutor {
    #[must_use]
    pub fn new(tools: &ToolRegistry) -> Self {
        let mut tool_specs = BTreeMap::new();
        for name in tools.tool_names() {
            tool_specs.insert(name.clone(), name.clone());
        }
        Self { tool_specs }
    }
}

impl ToolExecutor for SdkToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self.tool_specs.contains_key(tool_name) {
            // In SDK mode, tools are stubs by default. Real implementations
            // should be provided by a custom ToolExecutor wrapping tools::execute_tool.
            Err(ToolError::new(format!(
                "SDK stub: {tool_name} called with {input} — \
                 provide a custom ToolExecutor for real tool execution"
            )))
        } else {
            Err(ToolError::new(format!("unknown tool: {tool_name}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_registry_manages_tool_names() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin("read_file");
        registry.register_builtin("bash");

        assert!(registry.has_tool("read_file"));
        assert!(registry.has_tool("bash"));
        assert!(!registry.has_tool("nonexistent"));
        assert_eq!(registry.tool_names().len(), 2);
    }

    #[test]
    fn create_builtin_tools_includes_standard_tools() {
        let registry = create_builtin_tools();
        assert!(registry.has_tool("read_file"));
        assert!(registry.has_tool("bash"));
        assert!(registry.has_tool("Agent"));
        assert!(registry.tool_names().len() >= 10);
    }
}
