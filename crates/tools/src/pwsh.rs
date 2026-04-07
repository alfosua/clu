use async_trait::async_trait;
use clu_core::tools::Tool;
use serde_json::{json, Value};
use std::error::Error;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

pub struct PowerShellTool;

fn resolve_powershell() -> Option<&'static PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(detect).as_ref()
}

fn detect() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CLU_POWERSHELL") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }

    let path_sep = if cfg!(windows) { ';' } else { ':' };
    let exe_names: &[&str] = if cfg!(windows) {
        &["pwsh.exe", "powershell.exe"]
    } else {
        &["pwsh"]
    };

    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(path_sep) {
            for name in exe_names {
                let p = PathBuf::from(dir).join(name);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    #[cfg(windows)]
    {
        let well_known = [
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        ];
        for c in well_known {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    cmd.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

#[async_trait]
impl Tool for PowerShellTool {
    fn name(&self) -> &str {
        "pwsh"
    }

    fn description(&self) -> &str {
        "Execute a PowerShell command (pwsh preferred, falls back to Windows PowerShell). \
         Use this on Windows for native cmdlets like Get-ChildItem, Get-Process, etc. \
         Returns stdout and stderr."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The PowerShell command or script to execute"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String, Box<dyn Error + Send + Sync>> {
        let command = params["command"]
            .as_str()
            .ok_or("Missing or invalid 'command' parameter")?;

        let exe = match resolve_powershell() {
            Some(p) => p,
            None => {
                return Ok("error: no PowerShell found. Install PowerShell 7 (pwsh) or set \
                          CLU_POWERSHELL to a powershell executable."
                    .to_string());
            }
        };

        let mut cmd = Command::new(exe);
        cmd.arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(command)
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
