use async_trait::async_trait;
use serde_json::Value;
use std::error::Error;

/// Represents a callable tool by the LLM
#[async_trait]
pub trait Tool: Send + Sync {
    /// The name of the tool (e.g., "bash", "read")
    fn name(&self) -> &str;
    
    /// The description of the tool to be passed to the LLM
    fn description(&self) -> &str;
    
    /// The JSON schema of the tool's parameters
    fn parameters(&self) -> Value;
    
    /// Execute the tool with the given parameters
    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>>;
}
