use std::io::IsTerminal;

pub fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

#[derive(Clone, Copy)]
pub struct OutputConfig {
    pub json: bool,
    pub quiet: bool,
}

impl OutputConfig {
    pub fn new(json_flag: bool, quiet: bool) -> Self {
        let json = json_flag || !std::io::stdout().is_terminal();
        Self { json, quiet }
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
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(json_value).expect("failed to serialize JSON")
            );
        } else {
            self.print_message(human_message);
        }
    }

    pub fn should_show_spinner(&self) -> bool {
        !self.quiet && !self.json && std::io::stderr().is_terminal()
    }
}

pub use crate::api::exit_codes;

pub fn exit_code_for_error(err: &crate::api::Error) -> i32 {
    err.exit_code()
}
