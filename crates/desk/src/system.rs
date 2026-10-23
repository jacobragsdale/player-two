//! Running the system's own programs: git, and on Windows `wsl.exe`. On Windows no console window
//! flashes up for them, since the app has none of its own.

use std::process::Stdio;

use tokio::process::Command;

/// Windows' `CREATE_NO_WINDOW`: the child gets no console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command for `program`, with no window and no input.
#[must_use]
pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Runs a command to its end and gives back what it printed, or `None` when it could not run or
/// failed.
pub async fn output(mut command: Command) -> Option<String> {
    let output = command.output().await.ok()?;
    output.status.success().then(|| text(&output.stdout))
}

/// Reads a program's output: UTF-16 when it looks like it, as `wsl.exe` writes, UTF-8 otherwise.
#[must_use]
pub fn text(bytes: &[u8]) -> String {
    let wide = bytes.len() >= 2
        && bytes.len().is_multiple_of(2)
        && bytes.iter().skip(1).step_by(2).all(|byte| *byte == 0);
    if wide {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// The WSL distros installed, by name. None on a system without WSL.
pub async fn distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut list = command("wsl.exe");
    list.args(["--list", "--quiet"]);
    output(list)
        .await
        .map(|names| {
            names
                .lines()
                .map(|line| line.trim().trim_matches('\0').to_owned())
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// A command that runs `script` with bash as a login shell in a WSL distro, so that it finds what
/// the person's profile puts on the `PATH`.
#[must_use]
pub fn in_wsl(distro: &str, script: &str) -> Command {
    let mut command = command("wsl.exe");
    command.args(["--distribution", distro, "--exec", "bash", "-lc", script]);
    command
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn wsl_output_is_read_whether_wide_or_not() {
        let wide: Vec<u8> = "Ubuntu\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(text(&wide), "Ubuntu\r\n");
        assert_eq!(text(b"Ubuntu\n"), "Ubuntu\n");
    }
}
