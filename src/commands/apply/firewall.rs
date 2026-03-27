use std::collections::HashMap;

use crate::api::Error;
use crate::api::client::ProxmoxClient;

use super::manifest::{FirewallScope, Manifest, yaml_value_to_string};
use super::reconciler::{ApplyResult, ConfigChange, ReconcileAction, Reconciler, ResourceState};

pub struct FirewallReconciler;

impl FirewallReconciler {
    /// Build the API path for firewall rules based on scope and target.
    fn rules_path(manifest: &Manifest, node: Option<&str>) -> Result<String, Error> {
        let scope = manifest
            .scope
            .as_ref()
            .ok_or_else(|| Error::Config("firewall rule missing scope".to_string()))?;
        match scope {
            FirewallScope::Cluster => Ok("/cluster/firewall/rules".to_string()),
            FirewallScope::Node => {
                let target = manifest.target.as_ref().ok_or_else(|| {
                    Error::Config("node-scoped firewall rule missing target".to_string())
                })?;
                Ok(format!("/nodes/{target}/firewall/rules"))
            }
            FirewallScope::Vm => {
                let target = manifest.target.as_ref().ok_or_else(|| {
                    Error::Config("vm-scoped firewall rule missing target".to_string())
                })?;
                let vmid: u32 = target
                    .parse()
                    .map_err(|_| Error::Config(format!("invalid vmid target: {target}")))?;
                let node = node.ok_or_else(|| {
                    Error::Config("cannot resolve node for vm-scoped firewall rule".to_string())
                })?;
                Ok(format!("/nodes/{node}/qemu/{vmid}/firewall/rules"))
            }
            FirewallScope::Container => {
                let target = manifest.target.as_ref().ok_or_else(|| {
                    Error::Config("container-scoped firewall rule missing target".to_string())
                })?;
                let vmid: u32 = target
                    .parse()
                    .map_err(|_| Error::Config(format!("invalid vmid target: {target}")))?;
                let node = node.ok_or_else(|| {
                    Error::Config(
                        "cannot resolve node for container-scoped firewall rule".to_string(),
                    )
                })?;
                Ok(format!("/nodes/{node}/lxc/{vmid}/firewall/rules"))
            }
        }
    }

    /// Resolve the node for vm/container scoped rules.
    async fn resolve_node_for_scope(
        client: &ProxmoxClient,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let scope = manifest.scope.as_ref();
        match scope {
            Some(FirewallScope::Vm) | Some(FirewallScope::Container) => {
                if let Some(target) = &manifest.target {
                    let vmid: u32 = target
                        .parse()
                        .map_err(|_| Error::Config(format!("invalid vmid target: {target}")))?;
                    let node = client.resolve_node_for_vmid(vmid, global_node).await?;
                    Ok(Some(node))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}

impl Reconciler for FirewallReconciler {
    async fn get_current(
        &self,
        client: &ProxmoxClient,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> Result<Option<ResourceState>, Error> {
        let comment = manifest
            .config
            .get("comment")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // No comment means no matching key; always create
        let comment = match comment {
            Some(c) if !c.is_empty() => c,
            _ => return Ok(None),
        };

        let node = Self::resolve_node_for_scope(client, manifest, global_node).await?;
        let path = Self::rules_path(manifest, node.as_deref())?;
        let rules: Vec<serde_json::Value> = client.get(&path).await?;

        let matches: Vec<(u32, &serde_json::Value)> = rules
            .iter()
            .filter_map(|r| {
                let rc = r.get("comment").and_then(|v| v.as_str())?;
                if rc == comment {
                    let pos = r.get("pos").and_then(|v| v.as_u64())? as u32;
                    Some((pos, r))
                } else {
                    None
                }
            })
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => {
                let (pos, rule) = &matches[0];
                let mut config = HashMap::new();
                if let Some(obj) = rule.as_object() {
                    for (k, v) in obj {
                        if k == "pos" {
                            continue;
                        }
                        let val = match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                            other => other.to_string(),
                        };
                        config.insert(k.clone(), val);
                    }
                }
                Ok(Some(ResourceState {
                    vmid: None,
                    node,
                    power_state: None,
                    config,
                    position: Some(*pos),
                }))
            }
            _ => {
                let positions: Vec<String> = matches.iter().map(|(p, _)| p.to_string()).collect();
                Err(Error::Config(format!(
                    "multiple firewall rules with comment '{}' at positions: {}. Deduplicate comments manually.",
                    comment,
                    positions.join(", ")
                )))
            }
        }
    }

    fn diff(&self, current: Option<&ResourceState>, desired: &Manifest) -> ReconcileAction {
        let Some(current) = current else {
            let params: HashMap<String, String> = desired
                .config
                .iter()
                .map(|(k, v)| (k.clone(), yaml_value_to_string(v)))
                .collect();
            return ReconcileAction::Create { params };
        };

        let mut changes = Vec::new();
        for (key, desired_val) in &desired.config {
            let desired_str = yaml_value_to_string(desired_val);
            let current_val = current.config.get(key);
            match current_val {
                Some(cv) if *cv == desired_str => {}
                Some(cv) => changes.push(ConfigChange {
                    key: key.clone(),
                    old: Some(cv.clone()),
                    new: desired_str,
                }),
                None => changes.push(ConfigChange {
                    key: key.clone(),
                    old: None,
                    new: desired_str,
                }),
            }
        }

        if changes.is_empty() {
            ReconcileAction::NoOp
        } else {
            ReconcileAction::Update { changes }
        }
    }

    async fn apply(
        &self,
        client: &ProxmoxClient,
        action: &ReconcileAction,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> Result<ApplyResult, Error> {
        let node = Self::resolve_node_for_scope(client, manifest, global_node).await?;

        match action {
            ReconcileAction::NoOp => Ok(ApplyResult {
                vmid: None,
                message: "up to date".to_string(),
            }),
            ReconcileAction::Create { params } => {
                let path = Self::rules_path(manifest, node.as_deref())?;
                let param_list: Vec<(String, String)> =
                    params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                let param_refs: Vec<(&str, &str)> = param_list
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let _: serde_json::Value = client.post(&path, &param_refs).await?;
                let comment = manifest
                    .config
                    .get("comment")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no comment)");
                Ok(ApplyResult {
                    vmid: None,
                    message: format!("created rule: {comment}"),
                })
            }
            ReconcileAction::Update { changes } => {
                let current = self.get_current(client, manifest, global_node).await?;
                let pos = current.as_ref().and_then(|c| c.position).ok_or_else(|| {
                    Error::Other("cannot update rule: position unknown".to_string())
                })?;

                let path = format!("{}/{pos}", Self::rules_path(manifest, node.as_deref())?);

                let params: Vec<(String, String)> = manifest
                    .config
                    .iter()
                    .map(|(k, v)| (k.clone(), yaml_value_to_string(v)))
                    .collect();
                let param_refs: Vec<(&str, &str)> = params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let _: serde_json::Value = client.put(&path, &param_refs).await?;

                Ok(ApplyResult {
                    vmid: None,
                    message: format!("updated ({} changes)", changes.len()),
                })
            }
            _ => Ok(ApplyResult {
                vmid: None,
                message: "unsupported action for firewall rule".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manifest::ResourceKind;
    use super::*;

    fn make_firewall_manifest(config: Vec<(&str, &str)>) -> Manifest {
        Manifest {
            kind: ResourceKind::FirewallRule,
            name: None,
            vmid: None,
            node: None,
            state: None,
            scope: Some(FirewallScope::Cluster),
            target: None,
            config: config
                .into_iter()
                .map(|(k, v)| (k.to_string(), serde_yaml::Value::String(v.to_string())))
                .collect(),
        }
    }

    #[test]
    fn diff_create_when_no_current() {
        let manifest = make_firewall_manifest(vec![
            ("action", "ACCEPT"),
            ("type", "in"),
            ("comment", "test"),
        ]);
        let reconciler = FirewallReconciler;
        let action = reconciler.diff(None, &manifest);
        match action {
            ReconcileAction::Create { params } => {
                assert_eq!(params.get("action").unwrap(), "ACCEPT");
            }
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn diff_noop_when_matching() {
        let current = ResourceState {
            vmid: None,
            node: None,
            power_state: None,
            config: [
                ("action".to_string(), "ACCEPT".to_string()),
                ("type".to_string(), "in".to_string()),
                ("comment".to_string(), "test".to_string()),
            ]
            .into(),
            position: Some(0),
        };
        let manifest = make_firewall_manifest(vec![
            ("action", "ACCEPT"),
            ("type", "in"),
            ("comment", "test"),
        ]);
        let reconciler = FirewallReconciler;
        assert!(reconciler.diff(Some(&current), &manifest).is_noop());
    }

    #[test]
    fn diff_update_when_different() {
        let current = ResourceState {
            vmid: None,
            node: None,
            power_state: None,
            config: [
                ("action".to_string(), "DROP".to_string()),
                ("type".to_string(), "in".to_string()),
                ("comment".to_string(), "test".to_string()),
            ]
            .into(),
            position: Some(0),
        };
        let manifest = make_firewall_manifest(vec![
            ("action", "ACCEPT"),
            ("type", "in"),
            ("comment", "test"),
        ]);
        let reconciler = FirewallReconciler;
        match reconciler.diff(Some(&current), &manifest) {
            ReconcileAction::Update { changes } => {
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[0].key, "action");
            }
            other => panic!("expected Update, got {:?}", other),
        }
    }

    #[test]
    fn rules_path_cluster() {
        let m = make_firewall_manifest(vec![("action", "ACCEPT"), ("type", "in")]);
        let path = FirewallReconciler::rules_path(&m, None).unwrap();
        assert_eq!(path, "/cluster/firewall/rules");
    }

    #[test]
    fn rules_path_node() {
        let mut m = make_firewall_manifest(vec![("action", "ACCEPT"), ("type", "in")]);
        m.scope = Some(FirewallScope::Node);
        m.target = Some("pve1".to_string());
        let path = FirewallReconciler::rules_path(&m, None).unwrap();
        assert_eq!(path, "/nodes/pve1/firewall/rules");
    }

    #[test]
    fn rules_path_vm() {
        let mut m = make_firewall_manifest(vec![("action", "ACCEPT"), ("type", "in")]);
        m.scope = Some(FirewallScope::Vm);
        m.target = Some("100".to_string());
        let path = FirewallReconciler::rules_path(&m, Some("pve1")).unwrap();
        assert_eq!(path, "/nodes/pve1/qemu/100/firewall/rules");
    }
}
