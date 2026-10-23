//! Starting an agent with its standard input and output piped to the desk, and stopping it with
//! everything it started: on Windows the whole Job Object, elsewhere the whole process group.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use player_two_core::event::RunsIn;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::agents::Found;

/// How many of the agent's last lines on standard error are kept, to say why it failed.
const KEPT_LINES: usize = 20;

/// An agent's process, running.
pub struct Running {
    child: Box<dyn ChildWrapper>,
    errors: Arc<Mutex<VecDeque<String>>>,
}

/// An agent just started: its process, where the desk writes to it, and where it writes back.
pub type Started = (Running, ChildStdin, ChildStdout);

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("id", &self.child.id())
            .finish_non_exhaustive()
    }
}

/// Starts an agent in a folder: a checkout's, or for the machine the agent's home folder.
pub fn start(agent: &Found, folder: &str) -> anyhow::Result<Started> {
    let mut command = match &agent.runs_in {
        RunsIn::Windows => {
            let program = agent.program.clone();
            let args = agent.args.clone();
            let path = agent.path.clone();
            let folder = folder.to_owned();
            CommandWrap::with_new(program, move |command| {
                command.args(&args).env("PATH", &path).current_dir(&folder);
            })
        }
        RunsIn::Wsl(distro) => {
            let mut args = vec![
                "--distribution".to_owned(),
                distro.clone(),
                "--cd".to_owned(),
                folder.to_owned(),
                "--exec".to_owned(),
                "/usr/bin/env".to_owned(),
                format!("PATH={}", agent.path),
                agent.program.clone(),
            ];
            args.extend(agent.args.iter().cloned());
            CommandWrap::with_new("wsl.exe", move |command| {
                command.args(&args);
            })
        }
    };
    command
        .command_mut()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use process_wrap::tokio::{CreationFlags, JobObject};
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;
        command
            .wrap(CreationFlags(CREATE_NO_WINDOW))
            .wrap(JobObject);
    }
    #[cfg(unix)]
    command.wrap(process_wrap::tokio::ProcessGroup::leader());
    command.wrap(KillOnDrop);
    let mut child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("cannot start {}: {error}", agent.program))?;
    let stdin = child
        .stdin()
        .take()
        .ok_or_else(|| anyhow::anyhow!("the agent has no input"))?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| anyhow::anyhow!("the agent has no output"))?;
    let errors = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr().take() {
        let kept = Arc::clone(&errors);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
                if kept.len() == KEPT_LINES {
                    kept.pop_front();
                }
                kept.push_back(line);
            }
        });
    }
    Ok((Running { child, errors }, stdin, stdout))
}

impl Running {
    /// What the agent last wrote on standard error, oldest first.
    #[must_use]
    pub fn last_errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Stops the agent and everything it started, and waits for it to be gone.
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        Box::into_pin(self.child.kill()).await?;
        Ok(())
    }

    /// Whether the agent has ended by itself.
    pub fn has_ended(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

#[cfg(test)]
mod tests {
    use player_two_core::event::RunsIn;
    use tokio::io::AsyncBufReadExt;

    use super::start;
    use crate::agents::{AgentKind, Found};

    /// A program that starts a long-running child, prints the child's process id, and waits.
    fn parent() -> Found {
        let (program, args) = if cfg!(windows) {
            (
                "powershell.exe",
                vec![
                    "-NoProfile".to_owned(),
                    "-Command".to_owned(),
                    "$p = Start-Process -PassThru -NoNewWindow ping -ArgumentList '-n','120','127.0.0.1'; Write-Output $p.Id; Start-Sleep 120".to_owned(),
                ],
            )
        } else {
            (
                "sh",
                vec!["-c".to_owned(), "sleep 120 & echo $!; wait".to_owned()],
            )
        };
        Found {
            kind: AgentKind::Cursor,
            runs_in: RunsIn::Windows,
            program: program.to_owned(),
            args,
            path: std::env::var("PATH").unwrap_or_default(),
            home: std::env::temp_dir().to_string_lossy().into_owned(),
        }
    }

    /// Whether a process of that id is still there.
    async fn alive(id: &str) -> bool {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            (
                "tasklist",
                vec!["/FI".to_owned(), format!("PID eq {id}"), "/NH".to_owned()],
            )
        } else {
            (
                "ps",
                vec![
                    "-p".to_owned(),
                    id.to_owned(),
                    "-o".to_owned(),
                    "stat=".to_owned(),
                ],
            )
        };
        let output = tokio::process::Command::new(program)
            .args(args)
            .output()
            .await
            .expect("it runs");
        let said = String::from_utf8_lossy(&output.stdout);
        // A zombie is gone in all but name; tasklist names a live process by its image.
        said.lines().any(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('Z') && !line.starts_with("INFO:")
        })
    }

    #[tokio::test]
    async fn stopping_an_agent_stops_what_it_started() {
        let agent = parent();
        let (mut running, _stdin, stdout) = start(&agent, &agent.home).expect("it starts");
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        let child = tokio::time::timeout(std::time::Duration::from_secs(30), lines.next_line())
            .await
            .expect("the child's id arrives")
            .expect("it reads")
            .expect("a line");
        let child = child.trim().to_owned();
        assert!(alive(&child).await, "the grandchild runs");
        running.stop().await.expect("it stops");
        let mut gone = false;
        for _ in 0..50 {
            if !alive(&child).await {
                gone = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(gone, "the grandchild was stopped too");
    }
}
