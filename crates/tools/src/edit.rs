use crate::diff::unified;
use async_trait::async_trait;
use clu_core::tools::Tool;
use serde_json::{json, Value};
use std::error::Error;
use std::path::Path;

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace an exact substring in a file. `old_string` must occur exactly once \
         unless `replace_all` is true. Returns a unified diff of the change."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to edit" },
                "old_string": { "type": "string", "description": "Exact text to replace" },
                "new_string": { "type": "string", "description": "Replacement text" },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace every occurrence (default: false)"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>> {
        let path = params["path"].as_str().ok_or("missing 'path'")?;
        let old_string = params["old_string"].as_str().ok_or("missing 'old_string'")?;
        let new_string = params["new_string"].as_str().ok_or("missing 'new_string'")?;
        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        if old_string == new_string {
            return Ok("error: old_string and new_string are identical".into());
        }

        let p = Path::new(path);
        let original = tokio::fs::read_to_string(p)
            .await
            .map_err(|e| format!("could not read {path}: {e}"))?;

        let count = original.matches(old_string).count();
        if count == 0 {
            return Ok(format!("error: old_string not found in {path}"));
        }
        if count > 1 && !replace_all {
            return Ok(format!(
                "error: old_string occurs {count} times in {path}; pass replace_all=true \
                 or extend old_string with more context to make it unique"
            ));
        }

        let updated = if replace_all {
            original.replace(old_string, new_string)
        } else {
            original.replacen(old_string, new_string, 1)
        };

        tokio::fs::write(p, &updated).await?;
        Ok(unified(path, &original, &updated))
    }
}
