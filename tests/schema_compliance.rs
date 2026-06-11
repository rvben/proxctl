use serde_json::Value;

#[test]
fn schema_validates_against_clispec_v0_2() {
    let schema_str = include_str!("fixtures/clispec-v0.2.json");
    let meta_schema: Value = serde_json::from_str(schema_str).expect("parse clispec schema");

    use clap::Command;
    let cmd = Command::new("proxctl").subcommand(Command::new("test").about("test cmd"));

    let generated = proxctl::schema::generate(&cmd, &[]);

    let compiled = jsonschema::validator_for(&meta_schema).expect("compile meta schema");
    let errors: Vec<_> = compiled.iter_errors(&generated).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        panic!("clispec v0.2 validation failed:\n{}", msgs.join("\n"));
    }
}

#[test]
fn schema_commands_is_array() {
    use clap::Command;
    let cmd = Command::new("proxctl").subcommand(Command::new("test").about("test"));
    let schema = proxctl::schema::generate(&cmd, &[]);
    assert!(
        schema["commands"].is_array(),
        "commands must be a JSON array"
    );
}

#[test]
fn schema_output_fields_are_objects() {
    use clap::Command;
    let cmd = Command::new("proxctl").subcommand(Command::new("test").about("test"));
    let schema = proxctl::schema::generate(&cmd, &[]);
    for cmd_entry in schema["commands"].as_array().unwrap() {
        if let Some(fields) = cmd_entry.get("output_fields").and_then(|f| f.as_array()) {
            for field in fields {
                assert!(
                    field["name"].is_string(),
                    "output_field name must be string"
                );
                assert!(
                    field["type"].is_string(),
                    "output_field type must be string"
                );
            }
        }
    }
}

#[test]
fn schema_has_global_args_with_output() {
    use clap::Command;
    let cmd = Command::new("proxctl").subcommand(Command::new("test").about("t"));
    let schema = proxctl::schema::generate(&cmd, &[]);
    let global_args = schema["global_args"].as_array().unwrap();
    assert!(
        global_args.iter().any(|a| a["name"] == "--output"),
        "--output must be in global_args"
    );
}

#[test]
fn schema_errors_all_have_exit_codes() {
    use clap::Command;
    let cmd = Command::new("proxctl").subcommand(Command::new("test").about("t"));
    let schema = proxctl::schema::generate(&cmd, &[]);
    for error in schema["errors"].as_array().unwrap() {
        assert!(
            error.get("exit_code").and_then(|v| v.as_u64()).is_some(),
            "error {} missing exit_code",
            error["kind"]
        );
    }
}

#[test]
fn schema_subtree_narrowing_vm() {
    use clap::Command;
    let cmd = Command::new("proxctl")
        .subcommand(
            Command::new("vm")
                .subcommand(Command::new("list").about("list vms"))
                .subcommand(Command::new("start").about("start vm")),
        )
        .subcommand(Command::new("node").subcommand(Command::new("list").about("list nodes")));

    let schema = proxctl::schema::generate(&cmd, &["vm".to_string()]);
    let cmds = schema["commands"].as_array().unwrap();
    for cmd_entry in cmds {
        assert!(
            cmd_entry["name"].as_str().unwrap().starts_with("vm"),
            "expected only vm commands, got: {}",
            cmd_entry["name"]
        );
    }
}
