use crate::diff::unified;
use async_trait::async_trait;
use clu_core::tools::Tool;
use serde_json::{json, Value};
use std::error::Error;
use std::path::Path;

pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file (creating it or overwriting). Returns a unified diff \
         showing the change against the previous file contents."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "content": { "type": "string", "description": "Full new file contents" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>> {
        let path = params["path"].as_str().ok_or("missing 'path'")?;
        let content = params["content"].as_str().ok_or("missing 'content'")?;
        let p = Path::new(path);

        let old = tokio::fs::read_to_string(p).await.unwrap_or_default();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::write(p, content).await?;

        let diff = unified(path, &old, content);
        if diff.is_empty() {
            Ok(format!("{path}: no changes"))
        } else {
            Ok(diff)
        }
    }
}
