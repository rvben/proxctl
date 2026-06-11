use std::io::IsTerminal;

pub fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

#[derive(Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Auto,
    Text,
    Json,
}

#[derive(Clone, Copy)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub quiet: bool,
}

impl OutputConfig {
    pub fn new(format: OutputFormat, quiet: bool) -> Self {
        Self { format, quiet }
    }

    /// Returns true when JSON output should be used.
    ///
    /// Explicit Json/Text overrides TTY detection; Auto falls back to TTY check.
    pub fn is_json(&self) -> bool {
        match self.format {
            OutputFormat::Json => true,
            OutputFormat::Text => false,
            OutputFormat::Auto => !std::io::stdout().is_terminal(),
        }
    }

    /// Compatibility alias used throughout the codebase.
    pub fn json(&self) -> bool {
        self.is_json()
    }

    pub fn print_data(&self, data: &str) {
        println!("{data}");
    }

    pub fn print_message(&self, msg: &str) {
        if !self.quiet {
            eprintln!("{msg}");
        }
    }

    pub fn print_result(&self, json_value: &serde_json::Value, human_message: &str) {
        if self.is_json() {
            println!(
                "{}",
                serde_json::to_string_pretty(json_value).expect("failed to serialize JSON")
            );
        } else {
            self.print_message(human_message);
        }
    }

    pub fn should_show_spinner(&self) -> bool {
        !self.quiet && !self.is_json() && std::io::stderr().is_terminal()
    }
}

pub use crate::api::exit_codes;

pub fn exit_code_for_error(err: &crate::api::Error) -> i32 {
    err.exit_code()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_format_wins_json() {
        let cfg = OutputConfig::new(OutputFormat::Json, false);
        assert!(cfg.is_json(), "OutputFormat::Json must always return true");
    }

    #[test]
    fn explicit_format_wins_text() {
        let cfg = OutputConfig::new(OutputFormat::Text, false);
        assert!(
            !cfg.is_json(),
            "OutputFormat::Text must always return false"
        );
    }

    #[test]
    fn json_compat_alias() {
        let cfg = OutputConfig::new(OutputFormat::Json, false);
        assert_eq!(cfg.json(), cfg.is_json());
    }
}
