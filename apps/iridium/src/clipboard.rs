//! Clipboard policy and native transport. Terminal bytes belong to the driver.

/// Clipboard destination selected at startup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardMode {
    /// Native locally; terminal transport when the launcher detects SSH.
    #[default]
    Auto,
    /// Use the native system clipboard; failures are reported without fallback.
    System,
    /// Send copies through OSC 52; paste with the terminal's paste shortcut.
    Terminal,
    /// Keep text only inside this editor process.
    Internal,
}

impl ClipboardMode {
    /// Parses a CLI spelling without guessing unknown values.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "system" => Some(Self::System),
            "terminal" => Some(Self::Terminal),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }
}

/// The injectable native boundary. Tests never access a real pasteboard.
pub(crate) trait Provider {
    fn read(&mut self) -> Result<String, String>;
    fn write(&mut self, text: &str) -> Result<(), String>;
}

pub(crate) enum Backend {
    Internal,
    System(Box<dyn Provider>),
    Terminal,
}

impl Backend {
    pub(crate) fn new(mode: ClipboardMode, remote: bool) -> Self {
        match mode {
            ClipboardMode::Internal => Self::Internal,
            ClipboardMode::Terminal => Self::Terminal,
            ClipboardMode::Auto if remote => Self::Terminal,
            ClipboardMode::Auto | ClipboardMode::System => {
                Self::System(Box::new(Native::default()))
            },
        }
    }
}

/// Kept alive after first use, including on platforms where ownership serves data.
#[derive(Default)]
struct Native(Option<arboard::Clipboard>);

impl Native {
    fn handle(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.0.is_none() {
            self.0 = Some(
                arboard::Clipboard::new()
                    .map_err(|error| format!("Cannot reach the system clipboard: {error}"))?,
            );
        }
        self.0
            .as_mut()
            .ok_or_else(|| "System clipboard initialization failed".into())
    }
}

impl Provider for Native {
    fn read(&mut self) -> Result<String, String> {
        self.handle()?.get_text().map_err(|error| match error {
            arboard::Error::ContentNotAvailable => "The system clipboard holds no text".into(),
            other => format!("Cannot read the system clipboard: {other}"),
        })
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        self.handle()?
            .set_text(text)
            .map_err(|error| format!("Cannot write the system clipboard: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{self, Invocation};

    #[test]
    fn explicit_policy_overrides_ssh_without_opening_a_native_handle() {
        assert!(matches!(
            Backend::new(ClipboardMode::Auto, true),
            Backend::Terminal
        ));
        assert!(matches!(
            Backend::new(ClipboardMode::Auto, false),
            Backend::System(_)
        ));
        assert!(matches!(
            Backend::new(ClipboardMode::System, true),
            Backend::System(_)
        ));
        assert!(matches!(
            Backend::new(ClipboardMode::Internal, true),
            Backend::Internal
        ));
        assert!(matches!(
            Backend::new(ClipboardMode::Terminal, false),
            Backend::Terminal
        ));
    }

    #[test]
    fn cli_accepts_both_spellings_and_rejects_unknown_or_missing_modes() {
        for value in ["auto", "system", "terminal", "internal"] {
            for args in [
                vec!["--clipboard".to_owned(), value.to_owned()],
                vec![format!("--clipboard={value}")],
            ] {
                let invocation = cli::parse(args);
                assert!(
                    matches!(invocation, Ok(Invocation::Edit(options)) if Some(options.clipboard) == ClipboardMode::parse(value))
                );
            }
        }
        assert!(cli::parse(["--clipboard"]).is_err());
        assert!(cli::parse(["--clipboard="]).is_err());
        assert!(cli::parse(["--clipboard", "unknown"]).is_err());
    }
}
