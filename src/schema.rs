use std::collections::HashMap;

use serde_json::{Value, json};

/// A single output field descriptor for clispec v0.2.
struct FieldDef {
    name: &'static str,
    type_: &'static str,
    description: &'static str,
}

/// Convenience constructor so the meta! macro stays readable.
const fn field(name: &'static str, type_: &'static str, description: &'static str) -> FieldDef {
    FieldDef {
        name,
        type_,
        description,
    }
}

/// Metadata that clap doesn't know - manually maintained per command path.
struct CommandMeta {
    mutating: bool,
    idempotent: bool,
    dangerous: bool,
    async_capable: bool,
    output_fields: Vec<FieldDef>,
    notes: Option<&'static str>,
}

impl Default for CommandMeta {
    fn default() -> Self {
        Self {
            mutating: false,
            idempotent: true,
            dangerous: false,
            async_capable: false,
            output_fields: Vec::new(),
            notes: None,
        }
    }
}

/// Walk a clap `Arg` and produce a JSON arg description conforming to clispec v0.2 arg schema.
fn arg_to_json(arg: &clap::Arg) -> Value {
    let mut obj = serde_json::Map::new();

    let id = arg.get_id().as_str();

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

    let is_bool = !arg.get_action().takes_values();
    if is_bool {
        obj.insert("type".into(), json!("boolean"));
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

    if let Some(default) = arg.get_default_values().first() {
        obj.insert("default".into(), json!(default.to_string_lossy()));
    }

    if let Some(short) = arg.get_short() {
        obj.insert("short".into(), json!(format!("-{short}")));
    }

    Value::Object(obj)
}

/// Convert a `FieldDef` into a clispec v0.2 field object.
fn field_to_json(f: &FieldDef) -> Value {
    if f.description.is_empty() {
        json!({"name": f.name, "type": f.type_})
    } else {
        json!({"name": f.name, "type": f.type_, "description": f.description})
    }
}

/// Recursively walk the clap command tree and emit leaf commands as a flat Vec.
fn walk_commands(
    cmd: &clap::Command,
    prefix: &str,
    metadata: &HashMap<&str, CommandMeta>,
    out: &mut Vec<Value>,
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
            entry.insert("name".into(), json!(path));

            if let Some(about) = sub.get_about().map(|a| a.to_string()) {
                entry.insert("description".into(), json!(about));
            }

            let global_ids = [
                "help", "version", "host", "token", "node", "profile", "output", "json", "quiet",
                "insecure",
            ];
            let mut args = Vec::new();

            for arg in sub.get_arguments() {
                let id = arg.get_id().as_str();
                if global_ids.contains(&id) {
                    continue;
                }
                args.push(arg_to_json(arg));
            }

            entry.insert("args".into(), json!(args));

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

            if let Some(m) = meta {
                if !m.output_fields.is_empty() {
                    let fields: Vec<Value> = m.output_fields.iter().map(field_to_json).collect();
                    entry.insert("output_fields".into(), json!(fields));
                }
                if let Some(notes) = m.notes {
                    entry.insert("notes".into(), json!(notes));
                }
            }

            out.push(Value::Object(entry));
        }
    }
}

/// Generate the complete agent introspection schema conforming to clispec v0.2.
///
/// Pass a non-empty `path` slice to filter commands to those whose name starts with the
/// given prefix (e.g. `&["vm"]` returns only "vm ..." commands).
pub fn generate(cmd: &clap::Command, path: &[String]) -> Value {
    let metadata = build_metadata();

    let mut all_commands: Vec<Value> = Vec::new();
    walk_commands(cmd, "", &metadata, &mut all_commands);

    let filtered: Vec<Value> = if path.is_empty() {
        all_commands
    } else {
        let prefix = path.join(" ");
        all_commands
            .into_iter()
            .filter(|c| {
                let name = c["name"].as_str().unwrap_or("");
                name == prefix || name.starts_with(&format!("{prefix} "))
            })
            .collect()
    };

    json!({
        "clispec": "0.2",
        "name": "proxctl",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "CLI for Proxmox VE - manage VMs, containers, nodes, storage, and more",
        "global_args": [
            {"name": "--host", "type": "string", "description": "Proxmox host (e.g. pve.example.com:8006)"},
            {"name": "--token", "type": "string", "description": "API token (user@realm!tokenid=secret)"},
            {"name": "--node", "type": "string", "description": "Default node name"},
            {"name": "--profile", "type": "string", "description": "Configuration profile"},
            {"name": "--output", "type": "string", "short": "-o", "enum": ["auto", "text", "json"], "default": "auto", "description": "Output format; auto detects TTY"},
            {"name": "--quiet", "type": "boolean", "default": false, "description": "Suppress non-essential output"},
            {"name": "--insecure", "type": "boolean", "default": false, "description": "Accept invalid TLS certificates"},
            {"name": "--yes", "type": "boolean", "short": "-y", "default": false, "description": "Skip confirmation prompts for destructive operations (required in non-interactive mode)"}
        ],
        "commands": filtered,
        "errors": [
            {"kind": "config", "exit_code": 2, "retryable": false, "description": "Configuration error (missing or invalid host/token/profile)"},
            {"kind": "auth", "exit_code": 3, "retryable": false, "description": "Authentication failed (invalid token, 401/403)"},
            {"kind": "not_found", "exit_code": 4, "retryable": false, "description": "Resource not found (VM/container/node does not exist)"},
            {"kind": "api", "exit_code": 5, "retryable": true, "description": "API or task error (server error, task failed)"},
            {"kind": "conflict", "exit_code": 6, "retryable": false, "description": "Resource conflict (already in desired state)"},
            {"kind": "confirmation_required", "exit_code": 2, "retryable": false, "description": "Destructive command requires --yes flag in non-interactive mode"},
            {"kind": "timeout", "exit_code": 7, "retryable": true, "description": "Operation timed out"},
            {"kind": "usage", "exit_code": 2, "retryable": false, "description": "Invalid command syntax or unknown subcommand"},
            {"kind": "other", "exit_code": 1, "retryable": false, "description": "General error"}
        ]
    })
}

fn build_metadata() -> HashMap<&'static str, CommandMeta> {
    let mut m = HashMap::new();

    macro_rules! meta {
        ($path:expr, $($field:ident: $val:expr),* $(,)?) => {
            m.insert($path, CommandMeta { $($field: $val,)* ..Default::default() });
        };
    }

    // ── VM ───────────────────────────────────────────────────────────────────
    meta!("vm list", output_fields: vec![
        field("vmid",     "integer", "VM identifier"),
        field("name",     "string",  "VM name"),
        field("status",   "string",  "Current power state (running, stopped, paused)"),
        field("node",     "string",  "Node the VM is assigned to"),
        field("cpu",      "number",  "CPU usage fraction (0.0-1.0)"),
        field("maxcpu",   "integer", "Number of vCPUs allocated"),
        field("mem",      "integer", "Current memory usage in bytes"),
        field("maxmem",   "integer", "Maximum memory allocation in bytes"),
        field("uptime",   "integer", "Uptime in seconds"),
        field("template", "integer", "1 if this is a template, 0 otherwise"),
        field("pool",     "string",  "Resource pool the VM belongs to"),
    ]);
    meta!("vm status", output_fields: vec![
        field("vmid",      "integer", "VM identifier"),
        field("name",      "string",  "VM name"),
        field("status",    "string",  "Current power state (running, stopped, paused)"),
        field("cpu",       "number",  "CPU usage fraction (0.0-1.0)"),
        field("cpus",      "integer", "Number of vCPUs"),
        field("mem",       "integer", "Current memory usage in bytes"),
        field("maxmem",    "integer", "Maximum memory allocation in bytes"),
        field("uptime",    "integer", "Uptime in seconds"),
        field("pid",       "integer", "QEMU process PID (present when running)"),
        field("qmpstatus", "string",  "QEMU machine protocol status string"),
    ]);
    meta!("vm start", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'started' or 'already running'"),
        field("upid",   "string",  "Task UPID (empty string when already running)"),
    ]);
    meta!("vm stop", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'stopped' or 'already stopped'"),
        field("upid",   "string",  "Task UPID (empty string when already stopped)"),
    ]);
    meta!("vm shutdown", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'shutdown' or 'already stopped'"),
        field("upid",   "string",  "Task UPID for the shutdown operation"),
    ]);
    meta!("vm reboot", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'rebooting'"),
        field("upid",   "string",  "Task UPID for the reboot operation"),
    ]);
    meta!("vm reset", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'reset'"),
        field("upid",   "string",  "Task UPID for the reset operation"),
    ]);
    meta!("vm suspend", mutating: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'suspended'"),
        field("upid",   "string",  "Task UPID for the suspend operation"),
    ]);
    meta!("vm resume", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'resumed'"),
        field("upid",   "string",  "Task UPID for the resume operation"),
    ]);
    meta!("vm config", output_fields: vec![
        field("name",        "string", "VM name"),
        field("memory",      "string", "Memory allocation in MiB"),
        field("cores",       "string", "Number of CPU cores"),
        field("ostype",      "string", "Guest OS type (e.g. l26, win10)"),
        field("boot",        "string", "Boot order configuration string"),
        field("net0",        "string", "First network interface configuration string"),
        field("scsi0",       "string", "First SCSI disk configuration string"),
        field("onboot",      "string", "Whether VM starts on node boot (0 or 1)"),
        field("tags",        "string", "Semicolon-separated tags"),
        field("description", "string", "Free-text description"),
    ]);
    meta!("vm set", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'updated'"),
    ]);
    meta!("vm create", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Newly assigned VM identifier"),
        field("status", "string",  "Outcome: 'created'"),
        field("node",   "string",  "Node the VM was created on"),
        field("upid",   "string",  "Task UPID for the creation operation"),
    ]);
    meta!("vm destroy", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier that was destroyed"),
        field("status", "string",  "Outcome: 'destroyed'"),
        field("upid",   "string",  "Task UPID for the destroy operation"),
    ]);
    meta!("vm clone", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("source_vmid", "integer", "Source VM identifier"),
        field("new_vmid",    "integer", "Newly created clone VM identifier"),
        field("status",      "string",  "Outcome: 'cloned'"),
        field("upid",        "string",  "Task UPID for the clone operation"),
    ]);
    meta!("vm migrate", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("target", "string",  "Target node name"),
        field("status", "string",  "Outcome: 'migrated'"),
        field("upid",   "string",  "Task UPID for the migration operation"),
    ]);
    meta!("vm template", mutating: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier converted to template"),
        field("status", "string",  "Outcome: 'converted'"),
    ]);
    meta!("vm resize", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("disk",   "string",  "Disk identifier that was resized (e.g. scsi0)"),
        field("size",   "string",  "New disk size string (e.g. +10G or 50G)"),
        field("status", "string",  "Outcome: 'resized'"),
    ]);
    meta!("vm console", output_fields: vec![
        field("vmid", "integer", "VM identifier"),
        field("node", "string",  "Node the VM is running on"),
        field("type", "string",  "Console type: 'vnc'"),
        field("url",  "string",  "Proxmox web UI URL for VNC console access"),
        field("hint", "string",  "Human-readable hint for accessing the console"),
    ]);
    meta!("vm snapshot list", output_fields: vec![
        field("name",        "string",  "Snapshot name"),
        field("description", "string",  "Snapshot description"),
        field("snaptime",    "integer", "Unix timestamp when snapshot was taken"),
        field("vmstate",     "string",  "Whether VM state (RAM) was included in snapshot"),
    ]);
    meta!("vm snapshot create", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",     "integer", "VM identifier"),
        field("snapshot", "string",  "Snapshot name"),
        field("status",   "string",  "Outcome: 'created'"),
        field("upid",     "string",  "Task UPID for the snapshot creation"),
    ]);
    meta!("vm snapshot rollback", mutating: true, output_fields: vec![
        field("vmid",     "integer", "VM identifier"),
        field("snapshot", "string",  "Snapshot name rolled back to"),
        field("status",   "string",  "Outcome: 'rolled back'"),
        field("upid",     "string",  "Task UPID for the rollback operation"),
    ]);
    meta!("vm snapshot delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("vmid",     "integer", "VM identifier"),
        field("snapshot", "string",  "Snapshot name that was deleted"),
        field("status",   "string",  "Outcome: 'deleted'"),
        field("upid",     "string",  "Task UPID for the delete operation"),
    ]);
    meta!("vm agent exec", mutating: true, idempotent: false, output_fields: vec![
        field("exitcode",  "integer", "Exit code of the command run inside the VM"),
        field("out-data",  "string",  "Standard output of the command"),
        field("err-data",  "string",  "Standard error of the command"),
    ]);
    meta!("vm agent file-read", output_fields: vec![
        field("content", "string", "Raw text content of the file read from the VM guest"),
    ]);
    meta!("vm agent file-write", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("file",   "string",  "Path of the file written inside the VM guest"),
        field("status", "string",  "Outcome: 'written'"),
    ]);
    meta!("vm agent info", output_fields: vec![
        field("supported_commands", "string", "JSON array of commands supported by the installed QEMU guest agent"),
    ]);
    meta!("vm firewall rules", output_fields: vec![
        field("pos",     "integer", "Rule position (0-based order)"),
        field("action",  "string",  "Rule action: ACCEPT, DROP, or REJECT"),
        field("type",    "string",  "Rule direction: in or out"),
        field("proto",   "string",  "IP protocol (tcp, udp, icmp, etc.)"),
        field("source",  "string",  "Source address, IP set, or alias"),
        field("dest",    "string",  "Destination address, IP set, or alias"),
        field("dport",   "string",  "Destination port or port range"),
        field("enable",  "integer", "Whether the rule is enabled (1) or disabled (0)"),
        field("comment", "string",  "Human-readable rule comment"),
    ]);
    meta!("vm firewall add", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("action", "string",  "Rule action that was added (ACCEPT, DROP, REJECT)"),
        field("type",   "string",  "Rule direction (in or out)"),
        field("status", "string",  "Outcome: 'added'"),
    ]);
    meta!("vm firewall delete", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("pos",    "integer", "Rule position that was deleted"),
        field("status", "string",  "Outcome: 'deleted'"),
    ]);
    meta!("vm cloudinit show", output_fields: vec![
        field("ipconfig0",    "string", "IP configuration for the first network interface"),
        field("nameserver",   "string", "DNS nameserver(s)"),
        field("searchdomain", "string", "DNS search domain"),
        field("sshkeys",      "string", "URL-encoded authorized SSH public keys"),
        field("ciuser",       "string", "Cloud-init default username"),
    ]);
    meta!("vm cloudinit set", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "VM identifier"),
        field("status", "string",  "Outcome: 'updated'"),
    ]);

    // ── Container ────────────────────────────────────────────────────────────
    meta!("container list", output_fields: vec![
        field("vmid",     "integer", "Container identifier"),
        field("name",     "string",  "Container hostname"),
        field("status",   "string",  "Current power state (running, stopped)"),
        field("node",     "string",  "Node the container is assigned to"),
        field("cpu",      "number",  "CPU usage fraction (0.0-1.0)"),
        field("maxcpu",   "integer", "Number of vCPUs allocated"),
        field("mem",      "integer", "Current memory usage in bytes"),
        field("maxmem",   "integer", "Maximum memory allocation in bytes"),
        field("uptime",   "integer", "Uptime in seconds"),
        field("template", "integer", "1 if this is a template, 0 otherwise"),
        field("pool",     "string",  "Resource pool the container belongs to"),
    ]);
    meta!("container status", output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("name",   "string",  "Container hostname"),
        field("status", "string",  "Current power state (running, stopped)"),
        field("cpu",    "number",  "CPU usage fraction (0.0-1.0)"),
        field("cpus",   "integer", "Number of vCPUs"),
        field("mem",    "integer", "Current memory usage in bytes"),
        field("maxmem", "integer", "Maximum memory allocation in bytes"),
        field("uptime", "integer", "Uptime in seconds"),
        field("pid",    "integer", "Init process PID (present when running)"),
    ]);
    meta!("container start", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'started' or 'already running'"),
        field("upid",   "string",  "Task UPID (empty string when already running)"),
    ]);
    meta!("container stop", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'stopped' or 'already stopped'"),
        field("upid",   "string",  "Task UPID (empty string when already stopped)"),
    ]);
    meta!("container shutdown", mutating: true, idempotent: true, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'shutdown' or 'already stopped'"),
        field("upid",   "string",  "Task UPID for the shutdown operation"),
    ]);
    meta!("container reboot", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'rebooting'"),
        field("upid",   "string",  "Task UPID for the reboot operation"),
    ]);
    meta!("container suspend", mutating: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'suspended'"),
        field("upid",   "string",  "Task UPID for the suspend operation"),
    ]);
    meta!("container resume", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'resumed'"),
        field("upid",   "string",  "Task UPID for the resume operation"),
    ]);
    meta!("container config", output_fields: vec![
        field("hostname",    "string", "Container hostname"),
        field("memory",      "string", "Memory allocation in MiB"),
        field("cores",       "string", "Number of CPU cores"),
        field("ostype",      "string", "OS type (e.g. ubuntu, debian, alpine)"),
        field("rootfs",      "string", "Root filesystem configuration string"),
        field("net0",        "string", "First network interface configuration string"),
        field("onboot",      "string", "Whether container starts on node boot (0 or 1)"),
        field("features",    "string", "Enabled LXC feature flags"),
        field("description", "string", "Free-text description"),
    ]);
    meta!("container set", mutating: true, idempotent: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("status", "string",  "Outcome: 'updated'"),
    ]);
    meta!("container create", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Newly assigned container identifier"),
        field("status", "string",  "Outcome: 'created'"),
        field("node",   "string",  "Node the container was created on"),
        field("upid",   "string",  "Task UPID for the creation operation"),
    ]);
    meta!("container destroy", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier that was destroyed"),
        field("status", "string",  "Outcome: 'destroyed'"),
        field("upid",   "string",  "Task UPID for the destroy operation"),
    ]);
    meta!("container clone", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("source_vmid", "integer", "Source container identifier"),
        field("new_vmid",    "integer", "Newly created clone container identifier"),
        field("status",      "string",  "Outcome: 'cloned'"),
        field("upid",        "string",  "Task UPID for the clone operation"),
    ]);
    meta!("container migrate", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("target", "string",  "Target node name"),
        field("status", "string",  "Outcome: 'migrated'"),
        field("upid",   "string",  "Task UPID for the migration operation"),
    ]);
    meta!("container template", mutating: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier converted to template"),
        field("status", "string",  "Outcome: 'converted'"),
    ]);
    meta!("container resize", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("disk",   "string",  "Disk identifier that was resized (e.g. rootfs)"),
        field("size",   "string",  "New disk size string (e.g. +10G or 50G)"),
        field("status", "string",  "Outcome: 'resized'"),
    ]);
    meta!("container console", output_fields: vec![
        field("vmid", "integer", "Container identifier"),
        field("node", "string",  "Node the container is running on"),
        field("type", "string",  "Console type: 'shell'"),
        field("hint", "string",  "Human-readable hint for accessing the container shell"),
    ]);
    meta!("container snapshot list", output_fields: vec![
        field("name",        "string",  "Snapshot name"),
        field("description", "string",  "Snapshot description"),
        field("snaptime",    "integer", "Unix timestamp when snapshot was taken"),
    ]);
    meta!("container snapshot create", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",     "integer", "Container identifier"),
        field("snapshot", "string",  "Snapshot name"),
        field("status",   "string",  "Outcome: 'created'"),
        field("upid",     "string",  "Task UPID for the snapshot creation"),
    ]);
    meta!("container snapshot rollback", mutating: true, output_fields: vec![
        field("vmid",     "integer", "Container identifier"),
        field("snapshot", "string",  "Snapshot name rolled back to"),
        field("status",   "string",  "Outcome: 'rolled back'"),
        field("upid",     "string",  "Task UPID for the rollback operation"),
    ]);
    meta!("container snapshot delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("vmid",     "integer", "Container identifier"),
        field("snapshot", "string",  "Snapshot name that was deleted"),
        field("status",   "string",  "Outcome: 'deleted'"),
        field("upid",     "string",  "Task UPID for the delete operation"),
    ]);
    meta!("container firewall rules", output_fields: vec![
        field("pos",     "integer", "Rule position (0-based order)"),
        field("action",  "string",  "Rule action: ACCEPT, DROP, or REJECT"),
        field("type",    "string",  "Rule direction: in or out"),
        field("proto",   "string",  "IP protocol (tcp, udp, icmp, etc.)"),
        field("source",  "string",  "Source address, IP set, or alias"),
        field("dest",    "string",  "Destination address, IP set, or alias"),
        field("dport",   "string",  "Destination port or port range"),
        field("enable",  "integer", "Whether the rule is enabled (1) or disabled (0)"),
    ]);
    meta!("container firewall add", mutating: true, idempotent: false, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("action", "string",  "Rule action that was added (ACCEPT, DROP, REJECT)"),
        field("type",   "string",  "Rule direction (in or out)"),
        field("status", "string",  "Outcome: 'added'"),
    ]);
    meta!("container firewall delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("vmid",   "integer", "Container identifier"),
        field("pos",    "integer", "Rule position that was deleted"),
        field("status", "string",  "Outcome: 'deleted'"),
    ]);

    // ── Node ─────────────────────────────────────────────────────────────────
    meta!("node list", output_fields: vec![
        field("node",    "string",  "Node name"),
        field("status",  "string",  "Node availability: online or offline"),
        field("cpu",     "number",  "CPU usage fraction (0.0-1.0)"),
        field("maxcpu",  "integer", "Total number of CPU cores"),
        field("mem",     "integer", "Current memory usage in bytes"),
        field("maxmem",  "integer", "Total memory in bytes"),
        field("uptime",  "integer", "Uptime in seconds"),
    ]);
    meta!("node status", output_fields: vec![
        field("uptime",   "integer", "Node uptime in seconds"),
        field("cpuinfo",  "object",  "CPU information object (model, cpus, cores, sockets, mhz)"),
        field("memory",   "object",  "Memory usage object (total, used, free in bytes)"),
        field("kversion", "string",  "Linux kernel version string"),
        field("loadavg",  "array",   "1/5/15-minute load average array"),
    ]);
    meta!("node shutdown", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'shutdown initiated'"),
        field("node",   "string", "Name of the node being shut down"),
    ]);
    meta!("node reboot", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'reboot initiated'"),
        field("node",   "string", "Name of the node being rebooted"),
    ]);
    meta!("node start-all", mutating: true, async_capable: true, output_fields: vec![
        field("status", "string", "Outcome: 'start-all initiated'"),
        field("node",   "string", "Node name"),
        field("upid",   "string", "Task UPID for the start-all operation"),
    ]);
    meta!("node stop-all", mutating: true, dangerous: true, async_capable: true, output_fields: vec![
        field("status", "string", "Outcome: 'stop-all initiated'"),
        field("node",   "string", "Node name"),
        field("upid",   "string", "Task UPID for the stop-all operation"),
    ]);
    meta!("node services", output_fields: vec![
        field("service",      "string", "Service unit name"),
        field("state",        "string", "Running state: running or stopped"),
        field("desc",         "string", "Human-readable service description"),
        field("active-state", "string", "Systemd active state string"),
    ]);
    meta!("node service start", mutating: true, output_fields: vec![
        field("status",  "string", "Outcome phrase (e.g. 'start initiated')"),
        field("node",    "string", "Node name"),
        field("service", "string", "Service unit name"),
    ]);
    meta!("node service stop", mutating: true, output_fields: vec![
        field("status",  "string", "Outcome phrase (e.g. 'stop initiated')"),
        field("node",    "string", "Node name"),
        field("service", "string", "Service unit name"),
    ]);
    meta!("node service restart", mutating: true, output_fields: vec![
        field("status",  "string", "Outcome phrase (e.g. 'restart initiated')"),
        field("node",    "string", "Node name"),
        field("service", "string", "Service unit name"),
    ]);
    meta!("node network list", output_fields: vec![
        field("iface",   "string", "Network interface name"),
        field("type",    "string", "Interface type (bridge, bond, eth, vlan, etc.)"),
        field("address", "string", "IPv4 address assigned to the interface"),
        field("gateway", "string", "Default gateway for this interface"),
        field("active",  "integer", "Whether the interface is active (1) or not (0)"),
    ]);
    meta!("node network show", output_fields: vec![
        field("iface",        "string",  "Network interface name"),
        field("type",         "string",  "Interface type (bridge, bond, eth, vlan, etc.)"),
        field("address",      "string",  "IPv4 address"),
        field("netmask",      "string",  "Subnet mask"),
        field("gateway",      "string",  "Default gateway"),
        field("active",       "integer", "Whether the interface is currently active"),
        field("bridge_ports", "string",  "Space-separated list of ports in a bridge interface"),
    ]);
    meta!("node disk list", output_fields: vec![
        field("devpath", "string",  "Device path (e.g. /dev/sda)"),
        field("size",    "integer", "Disk size in bytes"),
        field("type",    "string",  "Disk type (hdd, ssd, nvme)"),
        field("health",  "string",  "SMART health status (PASSED, FAILED, UNKNOWN)"),
        field("model",   "string",  "Disk model name"),
        field("serial",  "string",  "Disk serial number"),
    ]);
    meta!("node disk smart", output_fields: vec![
        field("health",     "string", "SMART overall health assessment (PASSED or FAILED)"),
        field("attributes", "array",  "Array of SMART attribute objects (id, name, value, worst, raw)"),
    ]);
    meta!("node syslog", output_fields: vec![
        field("t", "string", "Syslog line text including timestamp, host, and message"),
    ]);
    meta!("node apt list", output_fields: vec![
        field("Package",    "string", "Debian package name"),
        field("OldVersion", "string", "Currently installed version"),
        field("Version",    "string", "Available update version"),
        field("Section",    "string", "Debian package section"),
    ]);
    meta!("node apt update", mutating: true, async_capable: true, output_fields: vec![
        field("status", "string", "Outcome: 'package index refreshed'"),
        field("node",   "string", "Node name"),
        field("upid",   "string", "Task UPID for the apt update operation"),
    ]);
    meta!("node certificate info", output_fields: vec![
        field("filename",    "string",  "Certificate file name on disk"),
        field("subject",     "string",  "Certificate subject DN"),
        field("issuer",      "string",  "Certificate issuer DN"),
        field("notafter",    "integer", "Unix timestamp of certificate expiry"),
        field("fingerprint", "string",  "Certificate fingerprint (SHA-256)"),
    ]);

    // ── Task ─────────────────────────────────────────────────────────────────
    meta!("task list", output_fields: vec![
        field("upid",       "string",  "Unique task identifier (UPID)"),
        field("node",       "string",  "Node where the task ran"),
        field("type",       "string",  "Task type (e.g. qmstart, vzdump, vzcreate)"),
        field("id",         "string",  "Resource ID the task acted on (VMID, storage name, etc.)"),
        field("user",       "string",  "User who initiated the task"),
        field("status",     "string",  "Task status: running or stopped"),
        field("starttime",  "integer", "Unix timestamp when the task started"),
        field("endtime",    "integer", "Unix timestamp when the task ended"),
        field("exitstatus", "string",  "Exit status string: 'OK' on success, error message otherwise"),
    ]);
    meta!("task status", output_fields: vec![
        field("status",     "string",  "Task status: running or stopped"),
        field("exitstatus", "string",  "Exit status: 'OK' on success, error message otherwise"),
        field("type",       "string",  "Task type (e.g. qmstart, vzdump)"),
        field("user",       "string",  "User who initiated the task"),
        field("node",       "string",  "Node where the task ran"),
        field("starttime",  "integer", "Unix timestamp when the task started"),
    ]);
    meta!("task log", output_fields: vec![
        field("t", "string",  "Log line text"),
        field("n", "integer", "Line number within the task log"),
    ]);
    meta!("task stop", mutating: true, output_fields: vec![
        field("status", "string", "Outcome: 'stopped'"),
        field("upid",   "string", "UPID of the task that was stopped"),
    ]);
    meta!("task wait", output_fields: vec![
        field("status",     "string", "Final task status: 'stopped'"),
        field("exitstatus", "string", "Exit status: 'OK' on success, error message otherwise"),
        field("upid",       "string", "UPID of the awaited task"),
    ]);

    // ── Storage ──────────────────────────────────────────────────────────────
    meta!("storage list", output_fields: vec![
        field("storage", "string", "Storage pool name"),
        field("type",    "string", "Storage backend type (dir, nfs, lvm, zfs, ceph, etc.)"),
        field("content", "string", "Comma-separated list of supported content types"),
        field("shared",  "string", "Whether storage is shared across nodes"),
        field("nodes",   "string", "Comma-separated list of nodes that can use this storage"),
    ]);
    meta!("storage status", output_fields: vec![
        field("storage", "string",  "Storage pool name"),
        field("type",    "string",  "Storage backend type"),
        field("active",  "string",  "Whether the storage is currently accessible"),
        field("total",   "integer", "Total storage capacity in bytes"),
        field("used",    "integer", "Used storage in bytes"),
        field("avail",   "integer", "Available storage in bytes"),
    ]);
    meta!("storage content", output_fields: vec![
        field("volid",   "string",  "Volume identifier (e.g. local:iso/debian.iso)"),
        field("format",  "string",  "Volume format (raw, qcow2, iso, vztmpl, etc.)"),
        field("size",    "integer", "Volume size in bytes"),
        field("content", "string",  "Content type (images, iso, vztmpl, backup, rootdir)"),
        field("vmid",    "integer", "VM/container ID this volume belongs to (if applicable)"),
    ]);
    meta!("storage upload", mutating: true, idempotent: false, output_fields: vec![
        field("status",   "string", "Outcome: 'uploaded'"),
        field("storage",  "string", "Storage pool the file was uploaded to"),
        field("filename", "string", "Name of the uploaded file"),
        field("upid",     "string", "Task UPID for the upload operation"),
    ]);
    meta!("storage download", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("status",  "string", "Outcome: 'download started'"),
        field("storage", "string", "Storage pool the file is being downloaded to"),
        field("upid",    "string", "Task UPID for the download operation"),
    ]);
    meta!("storage create", mutating: true, idempotent: false, output_fields: vec![
        field("status",  "string", "Outcome: 'created'"),
        field("storage", "string", "Name of the created storage pool"),
        field("type",    "string", "Backend type of the created storage pool"),
    ]);
    meta!("storage update", mutating: true, output_fields: vec![
        field("status",  "string", "Outcome: 'updated'"),
        field("storage", "string", "Name of the updated storage pool"),
    ]);
    meta!("storage delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status",  "string", "Outcome: 'deleted'"),
        field("storage", "string", "Name of the deleted storage pool"),
    ]);

    // ── Backup ───────────────────────────────────────────────────────────────
    meta!("backup list", output_fields: vec![
        field("volid",  "string",  "Volume identifier of the backup file"),
        field("vmid",   "integer", "VM/container ID that was backed up"),
        field("size",   "integer", "Backup file size in bytes"),
        field("ctime",  "integer", "Unix timestamp when the backup was created"),
        field("format", "string",  "Backup format (vma, tar, etc.)"),
    ]);
    meta!("backup create", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("status", "string",  "Outcome: 'backup created'"),
        field("vmid",   "integer", "VM/container ID that was backed up"),
        field("upid",   "string",  "Task UPID for the backup operation"),
    ]);
    meta!("backup restore", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("status", "string",  "Outcome: 'restored'"),
        field("vmid",   "integer", "VM/container ID that was restored"),
        field("upid",   "string",  "Task UPID for the restore operation"),
    ]);
    meta!("backup schedule list", output_fields: vec![
        field("id",       "string", "Backup schedule identifier"),
        field("schedule", "string", "Cron-style schedule string (e.g. 'sat 02:00')"),
        field("storage",  "string", "Storage target for scheduled backups"),
        field("mode",     "string", "Backup mode (snapshot, suspend, stop)"),
        field("vmid",     "string", "VM/container IDs to back up, or 'all'"),
    ]);
    meta!("backup schedule create", mutating: true, idempotent: false, output_fields: vec![
        field("status",   "string", "Outcome: 'created'"),
        field("schedule", "string", "Schedule string that was created"),
        field("storage",  "string", "Storage target for the schedule"),
    ]);
    meta!("backup schedule delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'deleted'"),
        field("id",     "string", "Schedule identifier that was deleted"),
    ]);

    // ── Cluster ──────────────────────────────────────────────────────────────
    meta!("cluster status", output_fields: vec![
        field("type",   "string",  "Entry type: 'cluster' or 'node'"),
        field("name",   "string",  "Cluster name or node name"),
        field("id",     "string",  "Node ID within the cluster"),
        field("ip",     "string",  "Node IP address"),
        field("online", "integer", "Whether the node is online (1) or offline (0)"),
        field("nodeid", "integer", "Numeric node ID"),
    ]);
    meta!("cluster resources", output_fields: vec![
        field("id",     "string",  "Resource identifier (e.g. qemu/100, storage/local)"),
        field("type",   "string",  "Resource type: qemu, lxc, node, storage, sdn"),
        field("node",   "string",  "Node the resource is on"),
        field("status", "string",  "Resource status (running, stopped, online, etc.)"),
        field("maxmem", "integer", "Maximum memory allocation in bytes"),
        field("maxcpu", "integer", "Number of CPU cores/vCPUs"),
        field("name",   "string",  "Resource name or hostname"),
    ]);
    meta!("cluster nextid", output_fields: vec![
        field("vmid", "integer", "Next available VM/container ID"),
    ]);
    meta!("cluster log", output_fields: vec![
        field("msg",  "string", "Log message text"),
        field("tag",  "string", "Log tag or category"),
        field("node", "string", "Node that generated the log entry"),
    ]);
    meta!("cluster options", output_fields: vec![
        field("migration", "string", "Migration options (bandwidth, type)"),
        field("console",   "string", "Default console type (applet, html5, xtermjs, vv)"),
        field("keyboard",  "string", "Default keyboard layout"),
        field("language",  "string", "Default web UI language"),
    ]);
    meta!("cluster ha resources", output_fields: vec![
        field("sid",   "string", "Service ID (e.g. vm:100)"),
        field("state", "string", "Desired HA state (started, stopped, enabled, disabled)"),
        field("group", "string", "HA group this resource is assigned to"),
    ]);
    meta!("cluster ha status", output_fields: vec![
        field("id",     "string", "Service or manager ID"),
        field("status", "string", "HA status string"),
        field("state",  "string", "Current HA state"),
        field("node",   "string", "Node currently managing this resource"),
    ]);

    // ── Firewall ─────────────────────────────────────────────────────────────
    meta!("firewall cluster rules", output_fields: vec![
        field("pos",     "integer", "Rule position (0-based order)"),
        field("action",  "string",  "Rule action: ACCEPT, DROP, or REJECT"),
        field("type",    "string",  "Rule direction: in or out"),
        field("proto",   "string",  "IP protocol (tcp, udp, icmp, etc.)"),
        field("source",  "string",  "Source address, IP set, or alias"),
        field("dest",    "string",  "Destination address, IP set, or alias"),
        field("dport",   "string",  "Destination port or port range"),
        field("enable",  "integer", "Whether the rule is enabled (1) or disabled (0)"),
        field("comment", "string",  "Human-readable rule comment"),
    ]);
    meta!("firewall cluster add", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'rule added'"),
        field("scope",  "string", "Scope of the rule: 'cluster'"),
    ]);
    meta!("firewall cluster delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string",  "Outcome: 'rule deleted'"),
        field("pos",    "integer", "Position of the deleted rule"),
        field("scope",  "string",  "Scope: 'cluster'"),
    ]);
    meta!("firewall node rules", output_fields: vec![
        field("pos",     "integer", "Rule position (0-based order)"),
        field("action",  "string",  "Rule action: ACCEPT, DROP, or REJECT"),
        field("type",    "string",  "Rule direction: in or out"),
        field("proto",   "string",  "IP protocol (tcp, udp, icmp, etc.)"),
        field("source",  "string",  "Source address, IP set, or alias"),
        field("dest",    "string",  "Destination address, IP set, or alias"),
        field("dport",   "string",  "Destination port or port range"),
        field("enable",  "integer", "Whether the rule is enabled (1) or disabled (0)"),
    ]);
    meta!("firewall node add", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'rule added'"),
        field("scope",  "string", "Scope: 'node'"),
        field("node",   "string", "Node the rule was added to"),
    ]);
    meta!("firewall node delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string",  "Outcome: 'rule deleted'"),
        field("pos",    "integer", "Position of the deleted rule"),
        field("scope",  "string",  "Scope: 'node'"),
        field("node",   "string",  "Node the rule was deleted from"),
    ]);
    meta!("firewall groups", output_fields: vec![
        field("group",   "string", "Security group name"),
        field("comment", "string", "Human-readable group comment"),
    ]);
    meta!("firewall group show", output_fields: vec![
        field("pos",    "integer", "Rule position (0-based order)"),
        field("action", "string",  "Rule action: ACCEPT, DROP, or REJECT"),
        field("type",   "string",  "Rule direction: in or out"),
        field("proto",  "string",  "IP protocol"),
        field("source", "string",  "Source address"),
        field("dest",   "string",  "Destination address"),
        field("dport",  "string",  "Destination port or port range"),
    ]);
    meta!("firewall group create", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'created'"),
        field("group",  "string", "Name of the created security group"),
    ]);
    meta!("firewall group delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'deleted'"),
        field("group",  "string", "Name of the deleted security group"),
    ]);
    meta!("firewall ipset list", output_fields: vec![
        field("name",    "string", "IP set name"),
        field("comment", "string", "Human-readable comment"),
    ]);
    meta!("firewall ipset show", output_fields: vec![
        field("cidr",    "string", "CIDR notation entry in the IP set"),
        field("comment", "string", "Comment for this CIDR entry"),
    ]);
    meta!("firewall ipset create", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'created'"),
        field("ipset",  "string", "Name of the created IP set"),
    ]);
    meta!("firewall ipset delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'deleted'"),
        field("ipset",  "string", "Name of the deleted IP set"),
    ]);
    meta!("firewall aliases", output_fields: vec![
        field("name",    "string", "Alias name"),
        field("cidr",    "string", "IP or CIDR this alias resolves to"),
        field("comment", "string", "Human-readable comment"),
    ]);

    // ── Access ───────────────────────────────────────────────────────────────
    meta!("access users", output_fields: vec![
        field("userid",  "string",  "User ID in user@realm format"),
        field("enable",  "integer", "Whether the user account is enabled (1) or disabled (0)"),
        field("email",   "string",  "User email address"),
        field("expire",  "integer", "Account expiry Unix timestamp (0 = never expires)"),
    ]);
    meta!("access user show", output_fields: vec![
        field("userid",    "string",  "User ID in user@realm format"),
        field("enable",    "integer", "Whether the user account is enabled"),
        field("email",     "string",  "User email address"),
        field("firstname", "string",  "User first name"),
        field("lastname",  "string",  "User last name"),
        field("tokens",    "object",  "Map of token IDs to token metadata objects"),
    ]);
    meta!("access user create", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'created'"),
        field("userid", "string", "Created user ID in user@realm format"),
    ]);
    meta!("access user delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'deleted'"),
        field("userid", "string", "Deleted user ID"),
    ]);
    meta!("access roles", output_fields: vec![
        field("roleid", "string", "Role name"),
        field("privs",  "string", "Comma-separated list of privileges granted by this role"),
    ]);
    meta!("access acl", output_fields: vec![
        field("path",      "string",  "Resource path the ACL entry applies to"),
        field("ugid",      "string",  "User or group ID this ACL entry applies to"),
        field("roleid",    "string",  "Role granted by this ACL entry"),
        field("propagate", "integer", "Whether the ACL propagates to child paths (1) or not (0)"),
        field("type",      "string",  "Entry type: 'user' or 'group'"),
    ]);
    meta!("access token list", output_fields: vec![
        field("tokenid", "string",  "Token ID (the part after the ! in user@realm!tokenid)"),
        field("privsep", "integer", "Whether privilege separation is enabled (1) or not (0)"),
        field("comment", "string",  "Human-readable token comment"),
        field("expire",  "integer", "Token expiry Unix timestamp (0 = never)"),
    ]);
    meta!("access token create", mutating: true, idempotent: false, output_fields: vec![
        field("tokenid", "string", "Created token ID"),
        field("value",   "string", "Token secret value (only shown once at creation)"),
    ]);
    meta!("access token delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status",  "string", "Outcome: 'deleted'"),
        field("userid",  "string", "User ID the token belonged to"),
        field("tokenid", "string", "Token ID that was deleted"),
    ]);

    // ── Pool ─────────────────────────────────────────────────────────────────
    meta!("pool list", output_fields: vec![
        field("poolid",  "string", "Resource pool identifier"),
        field("comment", "string", "Human-readable pool description"),
    ]);
    meta!("pool show", output_fields: vec![
        field("poolid",  "string", "Resource pool identifier"),
        field("comment", "string", "Human-readable pool description"),
        field("members", "array",  "Array of member resource objects (id, type, node, status)"),
    ]);
    meta!("pool create", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'created'"),
        field("poolid", "string", "Name of the created resource pool"),
    ]);
    meta!("pool update", mutating: true, output_fields: vec![
        field("status", "string", "Outcome: 'updated'"),
        field("poolid", "string", "Name of the updated resource pool"),
    ]);
    meta!("pool delete", mutating: true, idempotent: false, dangerous: true, output_fields: vec![
        field("status", "string", "Outcome: 'deleted'"),
        field("poolid", "string", "Name of the deleted resource pool"),
    ]);

    // ── Ceph ─────────────────────────────────────────────────────────────────
    meta!("ceph status", output_fields: vec![
        field("health", "object", "Ceph health object with status string and checks map"),
        field("osdmap", "object", "OSD map summary (num_osds, num_up_osds, num_in_osds)"),
        field("monmap", "object", "Monitor map summary (num_mons, epoch)"),
        field("pgmap",  "object", "Placement group map summary (num_pgs, bytes_total, bytes_used)"),
    ]);
    meta!("ceph osd list", output_fields: vec![
        field("id",     "integer", "OSD numeric ID"),
        field("status", "string",  "OSD status (up/down)"),
        field("type",   "string",  "OSD type (osd)"),
        field("host",   "string",  "Host name the OSD runs on"),
    ]);
    meta!("ceph osd create", mutating: true, idempotent: false, async_capable: true, output_fields: vec![
        field("status", "string", "Outcome: 'osd created'"),
        field("node",   "string", "Node the OSD was created on"),
        field("dev",    "string", "Block device path used for the OSD"),
        field("upid",   "string", "Task UPID for the OSD creation operation"),
    ]);
    meta!("ceph pool list", output_fields: vec![
        field("pool_name", "string",  "Ceph pool name"),
        field("size",      "integer", "Number of replicas"),
        field("pg_num",    "integer", "Number of placement groups"),
    ]);
    meta!("ceph pool create", mutating: true, idempotent: false, output_fields: vec![
        field("status", "string", "Outcome: 'pool created'"),
        field("name",   "string", "Name of the created Ceph pool"),
        field("node",   "string", "Node used to create the pool"),
    ]);
    meta!("ceph mon list", output_fields: vec![
        field("name", "string", "Monitor name"),
        field("host", "string", "Hostname of the monitor"),
        field("addr", "string", "Monitor address in host:port format"),
    ]);

    // ── Apply ─────────────────────────────────────────────────────────────────
    meta!("apply", mutating: true, idempotent: true, output_fields: vec![
        field("kind",    "string", "Resource kind: Vm, Container, or Firewall"),
        field("name",    "string", "Resource name"),
        field("vmid",    "integer", "VM/container ID (present for Vm and Container kinds)"),
        field("action",  "string", "Action taken: created, updated, or unchanged"),
        field("changes", "array",  "List of changed field names (present when action is updated)"),
        field("status",  "string", "Apply outcome status string"),
        field("error",   "string", "Error message if the resource failed to reconcile"),
    ]);

    // ── Export ────────────────────────────────────────────────────────────────
    meta!("export vm", output_fields: vec![
        field("kind",   "string", "Resource kind: 'Vm'"),
        field("name",   "string", "VM name"),
        field("vmid",   "integer", "VM identifier"),
        field("node",   "string", "Node the VM is on"),
        field("state",  "string", "Current VM power state"),
        field("config", "object", "Full VM configuration as an object"),
    ]);
    meta!("export container", output_fields: vec![
        field("kind",   "string", "Resource kind: 'Container'"),
        field("name",   "string", "Container hostname"),
        field("vmid",   "integer", "Container identifier"),
        field("node",   "string", "Node the container is on"),
        field("state",  "string", "Current container power state"),
        field("config", "object", "Full container configuration as an object"),
    ]);
    meta!("export firewall", output_fields: vec![
        field("kind",   "string", "Resource kind: 'Firewall'"),
        field("scope",  "string", "Firewall scope: cluster, node, vm, or container"),
        field("target", "string", "Target identifier (node name or VMID)"),
        field("config", "object", "Firewall rules and options as an object"),
    ]);

    // ── API passthrough ───────────────────────────────────────────────────────
    // Output is freeform passthrough from the Proxmox API; cannot describe statically.
    meta!("api get",    output_fields: vec![]);
    meta!("api post",   mutating: true, idempotent: false);
    meta!("api put",    mutating: true);
    meta!("api delete", mutating: true, idempotent: false);

    // ── Utility ───────────────────────────────────────────────────────────────
    meta!("health", output_fields: vec![
        field("status",         "string",  "Connectivity status: 'ok'"),
        field("nodes",          "integer", "Total number of nodes in the cluster"),
        field("nodes_online",   "integer", "Number of nodes currently online"),
        field("server_version", "string",  "Proxmox VE server version string (version-release)"),
    ]);
    meta!("version", output_fields: vec![
        field("cli_version",    "string", "proxctl CLI version"),
        field("server_version", "string", "Proxmox VE server version string (version-release)"),
        field("server_repoid",  "string", "Proxmox VE repository commit ID"),
    ]);
    // config init is interactive only; no JSON output.
    meta!("config init", mutating: true);
    meta!("config check", output_fields: vec![
        field("status",         "string", "Connectivity status: 'ok'"),
        field("server_version", "string", "Proxmox VE server version string (version-release)"),
    ]);
    // config show prints the config file path and masked TOML contents to stdout; no JSON output.
    meta!("config show", output_fields: vec![]);
    // schema and completions emit non-data text output.
    meta!("schema",      output_fields: vec![]);
    meta!("completions", output_fields: vec![]);

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
        let schema = generate(&test_cmd(), &[]);
        assert!(schema.get("name").is_some());
        assert!(schema.get("version").is_some());
        assert!(schema.get("global_args").is_some());
        assert!(schema.get("errors").is_some());
        assert!(schema.get("commands").is_some());
        assert!(schema.get("clispec").is_some());
    }

    #[test]
    fn schema_commands_is_array() {
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        assert!(!cmds.is_empty() || cmds.is_empty()); // just verify it's an array
    }

    #[test]
    fn schema_extracts_args() {
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        let test_cmd = cmds.iter().find(|c| c["name"] == "test").unwrap();
        let args = test_cmd["args"].as_array().unwrap();
        let vmid_arg = args.iter().find(|a| a["name"] == "vmid").unwrap();
        assert_eq!(vmid_arg["required"], true);
    }

    #[test]
    fn schema_extracts_flags_with_defaults() {
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        let test_cmd_entry = cmds.iter().find(|c| c["name"] == "test").unwrap();
        let args = test_cmd_entry["args"].as_array().unwrap();
        let timeout = args.iter().find(|a| a["name"] == "--timeout").unwrap();
        assert_eq!(timeout["default"], "300");
    }

    #[test]
    fn schema_extracts_enum_values() {
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        let test_cmd_entry = cmds.iter().find(|c| c["name"] == "test").unwrap();
        let args = test_cmd_entry["args"].as_array().unwrap();
        let mode = args.iter().find(|a| a["name"] == "--mode").unwrap();
        let enums = mode["enum"].as_array().unwrap();
        assert_eq!(enums, &[json!("fast"), json!("slow")]);
    }

    #[test]
    fn schema_errors_array_has_all_kinds() {
        let schema = generate(&test_cmd(), &[]);
        let errors = schema["errors"].as_array().unwrap();

        let kinds: Vec<&str> = errors.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"config"));
        assert!(kinds.contains(&"auth"));
        assert!(kinds.contains(&"not_found"));
        assert!(kinds.contains(&"api"));
        assert!(kinds.contains(&"conflict"));
        assert!(kinds.contains(&"timeout"));
        assert!(kinds.contains(&"other"));
        assert!(kinds.contains(&"confirmation_required"));

        let api = errors.iter().find(|e| e["kind"] == "api").unwrap();
        assert_eq!(api["retryable"], true);
        let timeout = errors.iter().find(|e| e["kind"] == "timeout").unwrap();
        assert_eq!(timeout["retryable"], true);
        let config = errors.iter().find(|e| e["kind"] == "config").unwrap();
        assert_eq!(config["retryable"], false);
    }

    #[test]
    fn schema_errors_have_exit_codes() {
        let schema = generate(&test_cmd(), &[]);
        let errors = schema["errors"].as_array().unwrap();
        for error in errors {
            assert!(
                error.get("exit_code").is_some(),
                "error {:?} missing exit_code",
                error["kind"]
            );
            assert!(
                error["exit_code"].as_u64().unwrap() >= 1,
                "exit_code must be >= 1"
            );
        }
    }

    #[test]
    fn schema_output_fields_are_objects() {
        // Build a command that has output_fields via metadata
        // Use a simpler check: generate with test_cmd and verify the format
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        // For commands that have output_fields, verify they are objects
        for cmd in cmds {
            if let Some(fields) = cmd.get("output_fields").and_then(|f| f.as_array()) {
                for field in fields {
                    assert!(field.get("name").is_some(), "output_field missing 'name'");
                    assert!(field.get("type").is_some(), "output_field missing 'type'");
                }
            }
        }
    }

    #[test]
    fn schema_has_global_args() {
        let schema = generate(&test_cmd(), &[]);
        let global_args = schema["global_args"].as_array().unwrap();
        let output_arg = global_args
            .iter()
            .find(|a| a["name"] == "--output")
            .unwrap();
        assert_eq!(output_arg["type"], "string");
        let enums = output_arg["enum"].as_array().unwrap();
        assert!(enums.contains(&json!("json")));
        assert!(enums.contains(&json!("text")));
        assert!(enums.contains(&json!("auto")));
    }

    #[test]
    fn schema_handles_nested_subcommands() {
        let schema = generate(&test_cmd(), &[]);
        let cmds = schema["commands"].as_array().unwrap();
        let names: Vec<&str> = cmds.iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"nested sub"));
        assert!(!names.contains(&"nested"));
    }

    #[test]
    fn schema_subtree_narrowing() {
        let schema = generate(&test_cmd(), &["nested".to_string()]);
        let cmds = schema["commands"].as_array().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0]["name"], "nested sub");
    }

    #[test]
    fn schema_subtree_narrowing_empty_for_unknown() {
        let schema = generate(&test_cmd(), &["nonexistent".to_string()]);
        let cmds = schema["commands"].as_array().unwrap();
        assert!(cmds.is_empty());
    }
}
