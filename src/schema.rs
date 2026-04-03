use std::collections::HashMap;

use serde_json::{Value, json};

/// Metadata that clap doesn't know — manually maintained per command path.
struct CommandMeta {
    mutating: bool,
    idempotent: bool,
    dangerous: bool,
    async_capable: bool,
    output_fields: &'static [&'static str],
}

impl Default for CommandMeta {
    fn default() -> Self {
        Self {
            mutating: false,
            idempotent: true,
            dangerous: false,
            async_capable: false,
            output_fields: &[],
        }
    }
}

/// Walk a clap `Arg` and produce a JSON description.
fn arg_to_json(arg: &clap::Arg) -> Value {
    let mut obj = serde_json::Map::new();

    let id = arg.get_id().as_str();

    // Use long flag name if available, otherwise the positional name
    let name = if arg.is_positional() {
        id.to_string()
    } else {
        arg.get_long()
            .map(|l| format!("--{l}"))
            .unwrap_or_else(|| id.to_string())
    };
    obj.insert("name".into(), json!(name));

    if let Some(help) = arg.get_help().map(|h| h.to_string()) {
        obj.insert("description".into(), json!(help));
    }

    // Type inference
    let is_bool = !arg.get_action().takes_values();
    if is_bool {
        obj.insert("type".into(), json!("bool"));
    } else {
        let possible: Vec<String> = arg
            .get_possible_values()
            .iter()
            .map(|v| v.get_name().to_string())
            .collect();
        if !possible.is_empty() {
            obj.insert("type".into(), json!("string"));
            obj.insert("enum".into(), json!(possible));
        } else {
            // Infer type from value name hint
            let value_name = arg
                .get_value_names()
                .and_then(|names| names.first())
                .map(|n| n.to_string())
                .unwrap_or_default()
                .to_uppercase();
            let inferred_type = match value_name.as_str() {
                "VMID" | "SECS" | "TIMEOUT" | "N" | "POS" | "LINES" | "MAX" => "integer",
                _ => "string",
            };
            obj.insert("type".into(), json!(inferred_type));
        }
    }

    if arg.is_positional() {
        obj.insert("required".into(), json!(arg.is_required_set()));
    }

    // Default value
    if let Some(default) = arg.get_default_values().first() {
        obj.insert("default".into(), json!(default.to_string_lossy()));
    }

    // Short flag
    if let Some(short) = arg.get_short() {
        obj.insert("short".into(), json!(format!("-{short}")));
    }

    Value::Object(obj)
}

/// Recursively walk the clap command tree and emit leaf commands.
fn walk_commands(
    cmd: &clap::Command,
    prefix: &str,
    metadata: &HashMap<&str, CommandMeta>,
    out: &mut serde_json::Map<String, Value>,
) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }

        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };

        let has_subcommands = sub.get_subcommands().any(|s| s.get_name() != "help");

        if has_subcommands {
            walk_commands(sub, &path, metadata, out);
        } else {
            let mut entry = serde_json::Map::new();

            if let Some(about) = sub.get_about().map(|a| a.to_string()) {
                entry.insert("description".into(), json!(about));
            }

            // Split into positional args and flags, skipping global/internal args
            let global_ids = [
                "help", "version", "host", "token", "node", "profile", "json", "quiet", "insecure",
            ];
            let mut args = Vec::new();
            let mut flags = Vec::new();

            for arg in sub.get_arguments() {
                let id = arg.get_id().as_str();
                if global_ids.contains(&id) {
                    continue;
                }
                if arg.is_positional() {
                    args.push(arg_to_json(arg));
                } else {
                    flags.push(arg_to_json(arg));
                }
            }

            if !args.is_empty() {
                entry.insert("args".into(), json!(args));
            }
            if !flags.is_empty() {
                entry.insert("flags".into(), json!(flags));
            }

            // Merge manually maintained metadata
            let meta = metadata.get(path.as_str());
            entry.insert("mutating".into(), json!(meta.is_some_and(|m| m.mutating)));
            entry.insert(
                "idempotent".into(),
                json!(meta.is_none_or(|m| m.idempotent)),
            );
            entry.insert("dangerous".into(), json!(meta.is_some_and(|m| m.dangerous)));
            entry.insert(
                "async_capable".into(),
                json!(meta.is_some_and(|m| m.async_capable)),
            );

            if let Some(m) = meta
                && !m.output_fields.is_empty()
            {
                entry.insert("output_fields".into(), json!(m.output_fields));
            }

            out.insert(path, Value::Object(entry));
        }
    }
}

/// Generate the complete agent introspection schema.
///
/// Auto-derives command structure, arguments, types, and defaults from clap.
/// Merges with manually maintained metadata (mutating/idempotent/dangerous/output_fields).
pub fn generate(cmd: &clap::Command) -> Value {
    let metadata = build_metadata();

    let mut commands = serde_json::Map::new();
    walk_commands(cmd, "", &metadata, &mut commands);

    json!({
        "name": "proxctl",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "CLI for Proxmox VE — manage VMs, containers, nodes, storage, and more",
        "usage": "proxctl [OPTIONS] <COMMAND> [SUBCOMMAND] [ARGS]",
        "errors": [
            {"kind": "config", "retryable": false, "description": "Configuration error"},
            {"kind": "auth", "retryable": false, "description": "Authentication failed"},
            {"kind": "not_found", "retryable": false, "description": "Resource not found"},
            {"kind": "api", "retryable": true, "description": "API error"},
            {"kind": "conflict", "retryable": false, "description": "Resource conflict"},
            {"kind": "timeout", "retryable": true, "description": "Operation timed out"},
            {"kind": "other", "retryable": false, "description": "General error"}
        ],
        "global_flags": {
            "--host": {"type": "string", "env": "PROXMOX_HOST", "description": "Proxmox host (e.g., 192.168.1.25:8006)"},
            "--token": {"type": "string", "env": "PROXMOX_TOKEN", "description": "API token (user@realm!tokenid=secret)"},
            "--node": {"type": "string", "env": "PROXMOX_NODE", "description": "Override automatic node detection for VM/container commands"},
            "--profile": {"type": "string", "env": "PROXMOX_PROFILE", "description": "Configuration profile name from config file"},
            "--json": {"type": "bool", "description": "Force JSON output (auto-enabled when stdout is not a terminal)"},
            "--quiet": {"type": "bool", "description": "Suppress spinners, progress, and non-data output"},
            "--insecure": {"type": "bool", "description": "Accept invalid/self-signed TLS certificates"}
        },
        "exit_codes": {
            "0": "success",
            "1": "general error",
            "2": "configuration error (missing host/token)",
            "3": "authentication error (invalid token, 401/403)",
            "4": "not found (VM/container/resource does not exist)",
            "5": "API or task error (server error, task failed)",
            "6": "conflict (resource already in desired state — treat as success for idempotent commands)",
            "7": "timeout (task did not complete within --timeout seconds)"
        },
        "notes": {
            "auto_json": "JSON output is automatic when stdout is piped (not a TTY). Use --json to force on a TTY.",
            "node_resolution": "VM and container commands auto-detect the node via cluster resources. Use --node to override.",
            "async_tasks": "Mutating commands wait for completion by default. Use --async to return immediately with a task UPID.",
            "idempotent": "Commands marked idempotent return exit 0 when the desired state already exists (e.g., starting an already-running VM).",
            "dangerous": "Commands marked dangerous require --yes flag or interactive confirmation. In non-TTY mode, --yes is mandatory.",
            "api_escape_hatch": "Use 'proxctl api get/post/put/delete <path>' to access any Proxmox API endpoint not wrapped by a dedicated command."
        },
        "commands": commands
    })
}

fn build_metadata() -> HashMap<&'static str, CommandMeta> {
    let mut m = HashMap::new();

    // Helper macro to reduce boilerplate
    macro_rules! meta {
        ($path:expr, $($field:ident: $val:expr),* $(,)?) => {
            m.insert($path, CommandMeta { $($field: $val,)* ..Default::default() });
        };
    }

    // VM
    meta!("vm list", output_fields: &["vmid", "name", "status", "node", "cpu", "maxcpu", "mem", "maxmem", "uptime", "template", "pool"]);
    meta!("vm status", output_fields: &["vmid", "name", "status", "cpu", "cpus", "mem", "maxmem", "uptime", "pid", "qmpstatus"]);
    meta!("vm start", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("vm stop", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("vm shutdown", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("vm reboot", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("vm reset", mutating: true, idempotent: false, output_fields: &["vmid", "status"]);
    meta!("vm suspend", mutating: true, output_fields: &["vmid", "status"]);
    meta!("vm resume", mutating: true, idempotent: true, output_fields: &["vmid", "status"]);
    meta!("vm config", output_fields: &["name", "memory", "cores", "ostype", "boot", "net0", "scsi0", "onboot", "tags", "description"]);
    meta!("vm set", mutating: true, idempotent: true, output_fields: &["vmid", "status"]);
    meta!("vm create", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("vm destroy", mutating: true, idempotent: false, dangerous: true, output_fields: &["vmid", "status"]);
    meta!("vm clone", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "newid", "status", "upid"]);
    meta!("vm migrate", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "target", "status", "upid"]);
    meta!("vm template", mutating: true, output_fields: &["vmid", "status"]);
    meta!("vm resize", mutating: true, idempotent: false, output_fields: &["vmid", "disk", "size"]);
    meta!("vm console", output_fields: &["type", "host", "port", "ticket"]);
    meta!("vm snapshot list", output_fields: &["name", "description", "snaptime", "vmstate"]);
    meta!("vm snapshot create", mutating: true, idempotent: false, output_fields: &["vmid", "name", "upid"]);
    meta!("vm snapshot rollback", mutating: true, output_fields: &["vmid", "name", "upid"]);
    meta!("vm snapshot delete", mutating: true, idempotent: false, dangerous: true, output_fields: &["vmid", "name"]);
    meta!("vm agent exec", mutating: true, idempotent: false, output_fields: &["exitcode", "out-data", "err-data"]);
    meta!("vm agent file-read", output_fields: &["content"]);
    meta!("vm agent file-write", mutating: true, idempotent: true);
    meta!("vm agent info", output_fields: &["supported_commands"]);
    meta!("vm firewall rules", output_fields: &["pos", "action", "type", "proto", "source", "dest", "dport", "enable", "comment"]);
    meta!("vm firewall add", mutating: true, idempotent: false);
    meta!("vm firewall delete", mutating: true, idempotent: false);
    meta!("vm cloudinit show", output_fields: &["ipconfig0", "nameserver", "searchdomain", "sshkeys", "ciuser"]);
    meta!("vm cloudinit set", mutating: true, idempotent: true);

    // Container
    meta!("container list", output_fields: &["vmid", "name", "status", "node", "cpu", "maxcpu", "mem", "maxmem", "uptime", "template", "pool"]);
    meta!("container status", output_fields: &["vmid", "name", "status", "cpu", "cpus", "mem", "maxmem", "uptime", "pid"]);
    meta!("container start", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("container stop", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("container shutdown", mutating: true, idempotent: true, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("container reboot", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("container suspend", mutating: true, output_fields: &["vmid", "status"]);
    meta!("container resume", mutating: true, idempotent: true, output_fields: &["vmid", "status"]);
    meta!("container config", output_fields: &["hostname", "memory", "cores", "ostype", "rootfs", "net0", "onboot", "features", "description"]);
    meta!("container set", mutating: true, idempotent: true);
    meta!("container create", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "status", "upid"]);
    meta!("container destroy", mutating: true, idempotent: false, dangerous: true, output_fields: &["vmid", "status"]);
    meta!("container clone", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "newid", "status", "upid"]);
    meta!("container migrate", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "target", "status", "upid"]);
    meta!("container template", mutating: true);
    meta!("container resize", mutating: true, idempotent: false);
    meta!("container console", output_fields: &["type", "host", "port", "ticket"]);
    meta!("container snapshot list", output_fields: &["name", "description", "snaptime"]);
    meta!("container snapshot create", mutating: true, idempotent: false);
    meta!("container snapshot rollback", mutating: true);
    meta!("container snapshot delete", mutating: true, idempotent: false, dangerous: true);
    meta!("container firewall rules", output_fields: &["pos", "action", "type", "proto", "source", "dest", "dport", "enable"]);
    meta!("container firewall add", mutating: true, idempotent: false);
    meta!("container firewall delete", mutating: true, idempotent: false);

    // Node
    meta!("node list", output_fields: &["node", "status", "cpu", "maxcpu", "mem", "maxmem", "uptime"]);
    meta!("node status", output_fields: &["uptime", "cpuinfo", "memory", "kversion", "loadavg"]);
    meta!("node shutdown", mutating: true, idempotent: false, dangerous: true);
    meta!("node reboot", mutating: true, idempotent: false, dangerous: true);
    meta!("node start-all", mutating: true, async_capable: true, output_fields: &["upid"]);
    meta!("node stop-all", mutating: true, dangerous: true, async_capable: true, output_fields: &["upid"]);
    meta!("node services", output_fields: &["service", "state", "desc", "active-state"]);
    meta!("node service start", mutating: true);
    meta!("node service stop", mutating: true);
    meta!("node service restart", mutating: true);
    meta!("node network list", output_fields: &["iface", "type", "address", "gateway", "active"]);
    meta!("node network show", output_fields: &["iface", "type", "address", "netmask", "gateway", "active", "bridge_ports"]);
    meta!("node disk list", output_fields: &["devpath", "size", "type", "health", "model", "serial"]);
    meta!("node disk smart", output_fields: &["health", "attributes"]);
    meta!("node syslog", output_fields: &["t"]);
    meta!("node apt list", output_fields: &["Package", "OldVersion", "Version", "Section"]);
    meta!("node apt update", mutating: true, async_capable: true);
    meta!("node certificate info", output_fields: &["filename", "subject", "issuer", "notafter", "fingerprint"]);

    // Task
    meta!("task list", output_fields: &["upid", "node", "type", "id", "user", "status", "starttime", "endtime", "exitstatus"]);
    meta!("task status", output_fields: &["status", "exitstatus", "type", "user", "node", "starttime"]);
    meta!("task log", output_fields: &["t", "n"]);
    meta!("task stop", mutating: true);
    meta!("task wait", output_fields: &["status", "exitstatus", "upid"]);

    // Storage
    meta!("storage list", output_fields: &["storage", "type", "content", "shared", "nodes"]);
    meta!("storage status", output_fields: &["storage", "type", "active", "total", "used", "avail"]);
    meta!("storage content", output_fields: &["volid", "format", "size", "content", "vmid"]);
    meta!("storage upload", mutating: true, idempotent: false);
    meta!("storage download", mutating: true, idempotent: false, async_capable: true);
    meta!("storage create", mutating: true, idempotent: false);
    meta!("storage update", mutating: true);
    meta!("storage delete", mutating: true, idempotent: false, dangerous: true);

    // Backup
    meta!("backup list", output_fields: &["volid", "vmid", "size", "ctime", "format"]);
    meta!("backup create", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "upid"]);
    meta!("backup restore", mutating: true, idempotent: false, async_capable: true, output_fields: &["vmid", "upid"]);
    meta!("backup schedule list", output_fields: &["id", "schedule", "storage", "mode", "vmid"]);
    meta!("backup schedule create", mutating: true, idempotent: false);
    meta!("backup schedule delete", mutating: true, idempotent: false, dangerous: true);

    // Cluster
    meta!("cluster status", output_fields: &["type", "name", "id", "ip", "online", "nodeid"]);
    meta!("cluster resources", output_fields: &["id", "type", "node", "status", "maxmem", "maxcpu", "name"]);
    meta!("cluster nextid", output_fields: &["vmid"]);
    meta!("cluster log", output_fields: &["msg", "tag", "node"]);
    meta!("cluster options", output_fields: &["migration", "console", "keyboard", "language"]);
    meta!("cluster ha resources", output_fields: &["sid", "state", "group"]);
    meta!("cluster ha status", output_fields: &["id", "status", "state", "node"]);

    // Firewall
    meta!("firewall cluster rules", output_fields: &["pos", "action", "type", "proto", "source", "dest", "dport", "enable", "comment"]);
    meta!("firewall cluster add", mutating: true, idempotent: false);
    meta!("firewall cluster delete", mutating: true, idempotent: false, dangerous: true);
    meta!("firewall node rules", output_fields: &["pos", "action", "type", "proto", "source", "dest", "dport", "enable"]);
    meta!("firewall node add", mutating: true, idempotent: false);
    meta!("firewall node delete", mutating: true, idempotent: false, dangerous: true);
    meta!("firewall groups", output_fields: &["group", "comment"]);
    meta!("firewall group show", output_fields: &["pos", "action", "type", "proto", "source", "dest", "dport"]);
    meta!("firewall group create", mutating: true, idempotent: false);
    meta!("firewall group delete", mutating: true, idempotent: false, dangerous: true);
    meta!("firewall ipset list", output_fields: &["name", "comment"]);
    meta!("firewall ipset show", output_fields: &["cidr", "comment"]);
    meta!("firewall ipset create", mutating: true, idempotent: false);
    meta!("firewall ipset delete", mutating: true, idempotent: false, dangerous: true);
    meta!("firewall aliases", output_fields: &["name", "cidr", "comment"]);

    // Access
    meta!("access users", output_fields: &["userid", "enable", "email", "expire"]);
    meta!("access user show", output_fields: &["userid", "enable", "email", "firstname", "lastname", "tokens"]);
    meta!("access user create", mutating: true, idempotent: false);
    meta!("access user delete", mutating: true, idempotent: false, dangerous: true);
    meta!("access roles", output_fields: &["roleid", "privs"]);
    meta!("access acl", output_fields: &["path", "ugid", "roleid", "propagate", "type"]);
    meta!("access token list", output_fields: &["tokenid", "privsep", "comment", "expire"]);
    meta!("access token create", mutating: true, idempotent: false, output_fields: &["tokenid", "value"]);
    meta!("access token delete", mutating: true, idempotent: false, dangerous: true);

    // Pool
    meta!("pool list", output_fields: &["poolid", "comment"]);
    meta!("pool show", output_fields: &["poolid", "comment", "members"]);
    meta!("pool create", mutating: true, idempotent: false);
    meta!("pool update", mutating: true);
    meta!("pool delete", mutating: true, idempotent: false, dangerous: true);

    // Ceph
    meta!("ceph status", output_fields: &["health", "osdmap", "monmap", "pgmap"]);
    meta!("ceph osd list", output_fields: &["id", "status", "type", "host"]);
    meta!("ceph osd create", mutating: true, idempotent: false, async_capable: true);
    meta!("ceph pool list", output_fields: &["pool_name", "size", "pg_num"]);
    meta!("ceph pool create", mutating: true, idempotent: false);
    meta!("ceph mon list", output_fields: &["name", "host", "addr"]);

    // Apply
    meta!("apply", mutating: true, idempotent: true, output_fields: &["kind", "name", "vmid", "action", "changes", "status", "error"]);

    // Export
    meta!("export vm", output_fields: &["kind", "name", "vmid", "node", "state", "config"]);
    meta!("export container", output_fields: &["kind", "name", "vmid", "node", "state", "config"]);
    meta!("export firewall", output_fields: &["kind", "scope", "target", "config"]);

    // API passthrough
    meta!("api get", output_fields: &[]);
    meta!("api post", mutating: true, idempotent: false);
    meta!("api put", mutating: true);
    meta!("api delete", mutating: true, idempotent: false);

    // Utility
    meta!("health", output_fields: &["status", "nodes", "nodes_online", "server_version"]);
    meta!("version", output_fields: &["cli_version", "server_version", "server_repoid"]);
    meta!("config init", mutating: true);
    meta!("config check", output_fields: &["status", "server_version"]);
    meta!("config show", output_fields: &[]);
    meta!("schema", output_fields: &[]);
    meta!("completions", output_fields: &[]);

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cmd() -> clap::Command {
        clap::Command::new("proxctl")
            .subcommand(
                clap::Command::new("test")
                    .about("A test command")
                    .arg(clap::Arg::new("vmid").required(true).help("VM ID"))
                    .arg(
                        clap::Arg::new("timeout")
                            .long("timeout")
                            .default_value("300")
                            .help("Timeout"),
                    )
                    .arg(
                        clap::Arg::new("mode")
                            .long("mode")
                            .value_parser(["fast", "slow"])
                            .help("Mode"),
                    ),
            )
            .subcommand(
                clap::Command::new("nested")
                    .subcommand(clap::Command::new("sub").about("Nested subcommand")),
            )
    }

    #[test]
    fn schema_has_required_top_level_keys() {
        let schema = generate(&test_cmd());
        assert!(schema.get("name").is_some());
        assert!(schema.get("version").is_some());
        assert!(schema.get("global_flags").is_some());
        assert!(schema.get("exit_codes").is_some());
        assert!(schema.get("errors").is_some());
        assert!(schema.get("commands").is_some());
        assert!(schema.get("notes").is_some());
    }

    #[test]
    fn schema_extracts_positional_args() {
        let schema = generate(&test_cmd());
        let cmds = schema["commands"].as_object().unwrap();
        let test_cmd = cmds.get("test").unwrap();
        let args = test_cmd["args"].as_array().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0]["name"], "vmid");
        assert_eq!(args[0]["required"], true);
    }

    #[test]
    fn schema_extracts_flags_with_defaults() {
        let schema = generate(&test_cmd());
        let cmds = schema["commands"].as_object().unwrap();
        let test_cmd = cmds.get("test").unwrap();
        let flags = test_cmd["flags"].as_array().unwrap();

        let timeout = flags.iter().find(|f| f["name"] == "--timeout").unwrap();
        assert_eq!(timeout["default"], "300");
    }

    #[test]
    fn schema_extracts_enum_values() {
        let schema = generate(&test_cmd());
        let cmds = schema["commands"].as_object().unwrap();
        let test_cmd = cmds.get("test").unwrap();
        let flags = test_cmd["flags"].as_array().unwrap();

        let mode = flags.iter().find(|f| f["name"] == "--mode").unwrap();
        let enums = mode["enum"].as_array().unwrap();
        assert_eq!(enums, &[json!("fast"), json!("slow")]);
    }

    #[test]
    fn schema_errors_array_has_all_kinds() {
        let schema = generate(&test_cmd());
        let errors = schema["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 7);

        let kinds: Vec<&str> = errors.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"config"));
        assert!(kinds.contains(&"auth"));
        assert!(kinds.contains(&"not_found"));
        assert!(kinds.contains(&"api"));
        assert!(kinds.contains(&"conflict"));
        assert!(kinds.contains(&"timeout"));
        assert!(kinds.contains(&"other"));

        // Verify retryable fields
        let api = errors.iter().find(|e| e["kind"] == "api").unwrap();
        assert_eq!(api["retryable"], true);
        let timeout = errors.iter().find(|e| e["kind"] == "timeout").unwrap();
        assert_eq!(timeout["retryable"], true);
        let config = errors.iter().find(|e| e["kind"] == "config").unwrap();
        assert_eq!(config["retryable"], false);
    }

    #[test]
    fn schema_handles_nested_subcommands() {
        let schema = generate(&test_cmd());
        let cmds = schema["commands"].as_object().unwrap();
        assert!(cmds.contains_key("nested sub"));
        assert!(!cmds.contains_key("nested"));
    }
}
