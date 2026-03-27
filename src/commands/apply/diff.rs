use owo_colors::OwoColorize;
use serde_json::json;

use crate::output::use_color;

use super::manifest::SourcedManifest;
use super::reconciler::ReconcileAction;

/// Format a diff for terminal display. Writes to stderr.
pub fn format_diff(sm: &SourcedManifest, action: &ReconcileAction, vmid: Option<u32>) {
    let label = sm.label();
    let vmid_str = vmid.map(|v| format!(" (vmid {v})")).unwrap_or_default();
    let color = use_color();

    match action {
        ReconcileAction::NoOp => {
            eprintln!("{label}{vmid_str}: up to date");
        }
        ReconcileAction::Create { params } => {
            eprintln!("{label}:");
            print_line("+", "create", &format!("vmid: auto{vmid_str}"), color);
            for (k, v) in params {
                print_line("+", k, v, color);
            }
        }
        ReconcileAction::Update { changes } => {
            eprintln!("{label}{vmid_str}:");
            for c in changes {
                match &c.old {
                    Some(old) => print_line("~", &c.key, &format!("{old} -> {}", c.new), color),
                    None => print_line("+", &c.key, &c.new, color),
                }
            }
        }
        ReconcileAction::SetState { from, to } => {
            eprintln!("{label}{vmid_str}:");
            print_line("~", "state", &format!("{from} -> {to}"), color);
        }
        ReconcileAction::CreateAndSetState { params, state } => {
            eprintln!("{label}:");
            print_line("+", "create", &format!("vmid: auto{vmid_str}"), color);
            for (k, v) in params {
                print_line("+", k, v, color);
            }
            print_line("+", "state", state, color);
        }
    }
}

fn print_line(symbol: &str, key: &str, value: &str, color: bool) {
    let line = format!("  {symbol} {key}: {value}");
    if !color {
        eprintln!("{line}");
        return;
    }
    match symbol {
        "+" => eprintln!("{}", line.green()),
        "~" => eprintln!("{}", line.yellow()),
        "-" => eprintln!("{}", line.red()),
        _ => eprintln!("{line}"),
    }
}

/// Build a JSON value for one resource result.
pub fn result_json(
    sm: &SourcedManifest,
    action: &ReconcileAction,
    vmid: Option<u32>,
    status: &str,
    error: Option<&str>,
) -> serde_json::Value {
    let kind = match sm.manifest.kind {
        super::manifest::ResourceKind::Vm => "vm",
        super::manifest::ResourceKind::Container => "container",
        super::manifest::ResourceKind::FirewallRule => "firewall-rule",
    };

    let changes: Vec<serde_json::Value> = match action {
        ReconcileAction::Update { changes } => changes
            .iter()
            .map(|c| {
                json!({
                    "key": c.key,
                    "from": c.old,
                    "to": c.new,
                })
            })
            .collect(),
        _ => vec![],
    };

    let mut obj = json!({
        "kind": kind,
        "name": sm.manifest.name,
        "vmid": vmid,
        "action": action.action_label(),
        "changes": changes,
        "status": status,
    });

    if let Some(err) = error {
        obj["error"] = json!(err);
    }

    obj
}

#[cfg(test)]
mod tests {
    use super::super::manifest::*;
    use super::super::reconciler::ConfigChange;
    use super::*;
    use std::collections::HashMap;

    fn test_manifest(name: &str) -> SourcedManifest {
        SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::Vm,
                name: Some(name.to_string()),
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: HashMap::new(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        }
    }

    #[test]
    fn result_json_noop() {
        let sm = test_manifest("web-01");
        let action = ReconcileAction::NoOp;
        let j = result_json(&sm, &action, Some(100), "ok", None);
        assert_eq!(j["action"], "noop");
        assert_eq!(j["status"], "ok");
        assert_eq!(j["vmid"], 100);
    }

    #[test]
    fn result_json_update_with_changes() {
        let sm = test_manifest("web-01");
        let action = ReconcileAction::Update {
            changes: vec![ConfigChange {
                key: "memory".to_string(),
                old: Some("2048".to_string()),
                new: "4096".to_string(),
            }],
        };
        let j = result_json(&sm, &action, Some(100), "ok", None);
        assert_eq!(j["action"], "update");
        assert_eq!(j["changes"][0]["key"], "memory");
        assert_eq!(j["changes"][0]["from"], "2048");
        assert_eq!(j["changes"][0]["to"], "4096");
    }

    #[test]
    fn result_json_error() {
        let sm = test_manifest("web-01");
        let action = ReconcileAction::Create {
            params: HashMap::new(),
        };
        let j = result_json(&sm, &action, None, "error", Some("API error 500"));
        assert_eq!(j["status"], "error");
        assert_eq!(j["error"], "API error 500");
    }
}
