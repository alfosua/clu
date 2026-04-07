use async_trait::async_trait;
use clu_core::tools::Tool;
use serde_json::{json, Value};
use std::error::Error;
use tokio::fs;

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>> {
        let path = params["path"]
            .as_str()
            .ok_or("Missing or invalid 'path' parameter")?;

        let contents = fs::read_to_string(path).await?;
        Ok(contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_read_tool_execution() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test file contents").unwrap();
        
        let tool = ReadTool;
        let params = json!({
            "path": temp_file.path().to_str().unwrap()
        });
        
        let result = tool.execute(params).await.unwrap();
        assert_eq!(result, "test file contents");
    }

    #[tokio::test]
    async fn test_read_tool_not_found() {
        let tool = ReadTool;
        let params = json!({
            "path": "/path/that/does/not/exist/12345.txt"
        });
        
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }
}
