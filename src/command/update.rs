//! `tasu update`: upgrade using however tasu was installed, so the binary and
//! the package manager that owns it never disagree.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

pub fn update() -> Result<()> {
    let method = Install::detect();
    println!(
        "tasu {} (installed via {})",
        env!("CARGO_PKG_VERSION"),
        method.label()
    );
    match method {
        Install::Homebrew => run("brew", &["upgrade", TAP_FORMULA]),
        Install::Installer => {
            if cfg!(windows) {
                print!("{}", windows_installer_hint());
                Ok(())
            } else {
                run("sh", &["-c", &installer_pipe()])
            }
        }
        Install::Cargo => {
            // Windows cannot overwrite a running executable, so point at the
            // command instead of failing halfway through.
            if cfg!(windows) {
                println!("run this in a new terminal:");
                println!("  cargo install tasu --force");
                Ok(())
            } else {
                run("cargo", &["install", "tasu", "--force"])
            }
        }
        Install::Unknown => {
            println!("could not tell how tasu was installed; update it the way you installed it:");
            println!("  Homebrew         brew upgrade {TAP_FORMULA}");
            println!("  Shell installer  {}", installer_pipe());
            print!("{}", windows_installer_hint());
            println!("  Cargo            cargo install tasu --force");
            Ok(())
        }
    }
}

const TAP_FORMULA: &str = "mancuoj/tap/tasu";
const INSTALL_URL: &str = "https://github.com/mancuoj-collective/tasu/releases/latest/download";

fn installer_pipe() -> String {
    format!("curl --proto '=https' --tlsv1.2 -LsSf {INSTALL_URL}/tasu-installer.sh | sh")
}

fn windows_installer_hint() -> String {
    format!(
        "  Windows          powershell -ExecutionPolicy Bypass -c \"irm {INSTALL_URL}/tasu-installer.ps1 | iex\"\n"
    )
}

/// How the running tasu was installed, inferred from where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Install {
    Homebrew,
    Installer,
    Cargo,
    Unknown,
}

impl Install {
    fn detect() -> Self {
        let exe = std::env::current_exe().ok();
        let target = exe
            .as_deref()
            .and_then(|path| std::fs::read_link(path).ok());
        let has_receipt = receipt_path().is_some_and(|path| path.exists());
        Self::classify(exe.as_deref(), target.as_deref(), has_receipt)
    }

    /// Pure, so it can be tested without a particular executable.
    fn classify(exe: Option<&Path>, symlink_target: Option<&Path>, has_receipt: bool) -> Self {
        let paths: Vec<&Path> = exe.into_iter().chain(symlink_target).collect();
        let has = |needle: &str| paths.iter().any(|p| p.to_string_lossy().contains(needle));
        if has("Cellar") {
            Self::Homebrew
        } else if has_receipt {
            Self::Installer
        } else if has(".cargo/bin") || has(".cargo\\bin") {
            Self::Cargo
        } else {
            Self::Unknown
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Installer => "the installer",
            Self::Cargo => "Cargo",
            Self::Unknown => "an unknown method",
        }
    }
}

/// The install receipt the shell / PowerShell installer writes.
fn receipt_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?
    };
    Some(base.join("tasu").join("tasu-receipt.json"))
}

/// Run an updater and inherit its output, so the user sees exactly what the
/// package manager did.
fn run(program: &str, args: &[&str]) -> Result<()> {
    println!("$ {program} {}", args.join(" "));
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => anyhow::bail!("`{program}` failed ({status}); tasu was not updated"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("`{program}` was not found on PATH")
        }
        Err(err) => Err(err).with_context(|| format!("failed to run `{program}`")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Install;

    #[test]
    fn install_method_is_inferred_from_the_executable_path() {
        let cellar = Path::new("/opt/homebrew/Cellar/tasu/0.6.0/bin/tasu");
        assert_eq!(
            Install::classify(Some(cellar), None, false),
            Install::Homebrew
        );
        // A symlink at .../bin resolves into Cellar.
        assert_eq!(
            Install::classify(
                Some(Path::new("/opt/homebrew/bin/tasu")),
                Some(cellar),
                false
            ),
            Install::Homebrew
        );
        // The installer drops a receipt next to a ~/.cargo/bin binary.
        assert_eq!(
            Install::classify(Some(Path::new("/home/u/.cargo/bin/tasu")), None, true),
            Install::Installer
        );
        assert_eq!(
            Install::classify(Some(Path::new("/home/u/.cargo/bin/tasu")), None, false),
            Install::Cargo
        );
        assert_eq!(
            Install::classify(Some(Path::new("/usr/local/bin/tasu")), None, false),
            Install::Unknown
        );
    }
}
