use async_trait::async_trait;
use clu_core::tools::Tool;
use serde_json::{json, Value};
use std::error::Error;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

pub struct BashTool;

#[derive(Debug, Clone)]
enum BashKind {
    /// Direct executable path (Unix `/bin/bash`, Git Bash, MSYS2, etc.)
    Direct(PathBuf),
    /// `wsl.exe bash -c <cmd>`
    Wsl,
    /// No bash found on this system.
    Missing,
}

fn resolve_bash() -> &'static BashKind {
    static CACHE: OnceLock<BashKind> = OnceLock::new();
    CACHE.get_or_init(detect_bash)
}

fn detect_bash() -> BashKind {
    if let Ok(p) = std::env::var("CLU_BASH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return BashKind::Direct(p);
        }
    }

    #[cfg(unix)]
    {
        return BashKind::Direct(PathBuf::from("/bin/bash"));
    }

    #[cfg(windows)]
    {
        // Git Bash — most common on dev machines.
        let candidates = [
            std::env::var("ProgramFiles").ok().map(|p| format!("{p}\\Git\\bin\\bash.exe")),
            std::env::var("ProgramFiles").ok().map(|p| format!("{p}\\Git\\usr\\bin\\bash.exe")),
            std::env::var("ProgramFiles(x86)").ok().map(|p| format!("{p}\\Git\\bin\\bash.exe")),
            Some("C:\\Program Files\\Git\\bin\\bash.exe".to_string()),
            Some("C:\\msys64\\usr\\bin\\bash.exe".to_string()),
        ];
        for c in candidates.into_iter().flatten() {
            let p = PathBuf::from(&c);
            if p.exists() {
                return BashKind::Direct(p);
            }
        }
        // PATH lookup.
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(';') {
                let p = PathBuf::from(dir).join("bash.exe");
                if p.exists() {
                    return BashKind::Direct(p);
                }
            }
        }
        // WSL fallback.
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(';') {
                if PathBuf::from(dir).join("wsl.exe").exists() {
                    return BashKind::Wsl;
                }
            }
        }
        BashKind::Missing
    }
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    // CREATE_NO_WINDOW — tokio::process::Command exposes creation_flags on Windows.
    cmd.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command in the current working directory. Returns stdout and stderr."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>> {
        let command = params["command"]
            .as_str()
            .ok_or("Missing or invalid 'command' parameter")?;

        let mut cmd = match resolve_bash() {
            BashKind::Direct(path) => {
                let mut c = Command::new(path);
                c.arg("-c").arg(command);
                c
            }
            BashKind::Wsl => {
                let mut c = Command::new("wsl.exe");
                c.arg("bash").arg("-c").arg(command);
                c
            }
            BashKind::Missing => {
                return Ok("error: no bash found on this system. Install Git for Windows, \
                          enable WSL, or set CLU_BASH to a bash executable."
                    .to_string());
            }
        };

        cmd.env("LANG", "C.UTF-8")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        no_window(&mut cmd);

        let timeout_secs = std::env::var("CLU_BASH_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30u64);

        let output = match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await
        {
            Ok(res) => res?,
            Err(_) => {
                return Ok(format!(
                    "error: command timed out after {timeout_secs}s (set CLU_BASH_TIMEOUT to override)"
                ));
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&format!("STDOUT:\n{}\n", stdout));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("STDERR:\n{}\n", stderr));
        }

        if result.is_empty() {
            result.push_str("Command executed successfully with no output.");
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_tool_execution() {
        let tool = BashTool;
        let params = json!({
            "command": "echo 'hello from bash'"
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.contains("hello from bash"));
    }
}
