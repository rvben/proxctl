use std::collections::HashMap;

use crate::api::Error;
use crate::api::client::ProxmoxClient;

use super::manifest::{DesiredState, Manifest, yaml_value_to_string};
use super::reconciler::{ApplyResult, ConfigChange, ReconcileAction, Reconciler, ResourceState};

pub struct ContainerReconciler;

impl ContainerReconciler {
    /// Find a container by name in cluster resources. Returns (vmid, node) if found uniquely.
    async fn find_by_name(
        client: &ProxmoxClient,
        name: &str,
    ) -> Result<Option<(u32, String)>, Error> {
        let resources = client.get_cluster_resources(Some("vm")).await?;
        let matches: Vec<_> = resources
            .iter()
            .filter(|r| r.resource_type == "lxc" && r.name.as_deref() == Some(name))
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => {
                let r = &matches[0];
                Ok(Some((
                    r.vmid
                        .ok_or_else(|| Error::Other("resource missing vmid".to_string()))?,
                    r.node
                        .clone()
                        .ok_or_else(|| Error::Other("resource missing node".to_string()))?,
                )))
            }
            _ => {
                let ids: Vec<String> = matches
                    .iter()
                    .filter_map(|r| {
                        r.vmid
                            .map(|v| format!("{v} (node: {})", r.node.as_deref().unwrap_or("?")))
                    })
                    .collect();
                Err(Error::Config(format!(
                    "multiple containers named '{name}': {}. Add 'vmid' to disambiguate.",
                    ids.join(", ")
                )))
            }
        }
    }
}

impl Reconciler for ContainerReconciler {
    async fn get_current(
        &self,
        client: &ProxmoxClient,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> Result<Option<ResourceState>, Error> {
        let (vmid, node) = if let Some(vmid) = manifest.vmid {
            match client.resolve_node_for_vmid(vmid, global_node).await {
                Ok(n) => (vmid, n),
                Err(Error::NotFound(_)) => return Ok(None),
                Err(e) => return Err(e),
            }
        } else if let Some(name) = &manifest.name {
            match Self::find_by_name(client, name).await? {
                Some((vmid, node)) => (vmid, node),
                None => return Ok(None),
            }
        } else {
            return Ok(None);
        };

        let path = format!("/nodes/{node}/lxc/{vmid}/config");
        let config_data: serde_json::Value = client.get(&path).await?;

        let mut config = HashMap::new();
        if let Some(obj) = config_data.as_object() {
            for (k, v) in obj {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                    other => other.to_string(),
                };
                config.insert(k.clone(), val);
            }
        }

        let status_path = format!("/nodes/{node}/lxc/{vmid}/status/current");
        let status_data: serde_json::Value = client.get(&status_path).await?;
        let power_state = status_data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Some(ResourceState {
            vmid: Some(vmid),
            node: Some(node),
            power_state: Some(power_state),
            config,
            position: None,
        }))
    }

    fn diff(&self, current: Option<&ResourceState>, desired: &Manifest) -> ReconcileAction {
        let Some(current) = current else {
            let params: HashMap<String, String> = desired
                .config
                .iter()
                .map(|(k, v)| (k.clone(), yaml_value_to_string(v)))
                .collect();
            if let Some(state) = &desired.state {
                let state_str = match state {
                    DesiredState::Running => "running",
                    DesiredState::Stopped => "stopped",
                };
                return ReconcileAction::CreateAndSetState {
                    params,
                    state: state_str.to_string(),
                };
            }
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

        let state_change = desired.state.as_ref().and_then(|desired_state| {
            let desired_str = match desired_state {
                DesiredState::Running => "running",
                DesiredState::Stopped => "stopped",
            };
            let current_power = current.power_state.as_deref().unwrap_or("unknown");
            if current_power != desired_str {
                Some((current_power.to_string(), desired_str.to_string()))
            } else {
                None
            }
        });

        match (changes.is_empty(), state_change) {
            (true, None) => ReconcileAction::NoOp,
            (false, None) => ReconcileAction::Update { changes },
            (true, Some((from, to))) => ReconcileAction::SetState { from, to },
            (false, Some((from, to))) => {
                changes.push(ConfigChange {
                    key: "__state__".to_string(),
                    old: Some(from),
                    new: to,
                });
                ReconcileAction::Update { changes }
            }
        }
    }

    async fn apply(
        &self,
        client: &ProxmoxClient,
        action: &ReconcileAction,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> Result<ApplyResult, Error> {
        match action {
            ReconcileAction::NoOp => Ok(ApplyResult {
                vmid: manifest.vmid,
                message: "up to date".to_string(),
            }),
            ReconcileAction::Create { params } => {
                let node = match manifest.node.as_deref().or(global_node) {
                    Some(n) => n.to_string(),
                    None => {
                        let nodes = client.list_nodes().await?;
                        nodes
                            .first()
                            .map(|n| n.node.clone())
                            .ok_or_else(|| Error::Config("no nodes available".to_string()))?
                    }
                };

                let next_id: u32 = client.get("/cluster/nextid").await?;
                let mut api_params: Vec<(String, String)> =
                    vec![("vmid".to_string(), next_id.to_string())];
                if let Some(name) = &manifest.name {
                    api_params.push(("hostname".to_string(), name.clone()));
                }
                for (k, v) in params {
                    api_params.push((k.clone(), v.clone()));
                }

                let param_refs: Vec<(&str, &str)> = api_params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let path = format!("/nodes/{node}/lxc");
                client
                    .execute_task(&path, &param_refs, &node, 300, true, false)
                    .await?;

                Ok(ApplyResult {
                    vmid: Some(next_id),
                    message: format!("created (vmid {next_id})"),
                })
            }
            ReconcileAction::Update { changes } => {
                let vmid = manifest.vmid.ok_or_else(|| {
                    Error::Other("cannot update container without resolved vmid".to_string())
                })?;
                let node = client.resolve_node_for_vmid(vmid, global_node).await?;

                let config_changes: Vec<&ConfigChange> =
                    changes.iter().filter(|c| c.key != "__state__").collect();
                let state_change = changes.iter().find(|c| c.key == "__state__");

                if !config_changes.is_empty() {
                    let params: Vec<(String, String)> = config_changes
                        .iter()
                        .map(|c| (c.key.clone(), c.new.clone()))
                        .collect();
                    let param_refs: Vec<(&str, &str)> = params
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    let path = format!("/nodes/{node}/lxc/{vmid}/config");
                    let _: serde_json::Value = client.put(&path, &param_refs).await?;
                }

                if let Some(sc) = state_change {
                    apply_ct_state(client, vmid, &node, &sc.new).await?;
                }

                let change_count = config_changes.len();
                Ok(ApplyResult {
                    vmid: Some(vmid),
                    message: format!("updated ({change_count} changes)"),
                })
            }
            ReconcileAction::SetState { to, .. } => {
                let vmid = manifest.vmid.ok_or_else(|| {
                    Error::Other("cannot set state without resolved vmid".to_string())
                })?;
                let node = client.resolve_node_for_vmid(vmid, global_node).await?;
                apply_ct_state(client, vmid, &node, to).await?;
                Ok(ApplyResult {
                    vmid: Some(vmid),
                    message: format!("state -> {to}"),
                })
            }
            ReconcileAction::CreateAndSetState { params, state } => {
                let node = match manifest.node.as_deref().or(global_node) {
                    Some(n) => n.to_string(),
                    None => {
                        let nodes = client.list_nodes().await?;
                        nodes
                            .first()
                            .map(|n| n.node.clone())
                            .ok_or_else(|| Error::Config("no nodes available".to_string()))?
                    }
                };

                let next_id: u32 = client.get("/cluster/nextid").await?;
                let mut api_params: Vec<(String, String)> =
                    vec![("vmid".to_string(), next_id.to_string())];
                if let Some(name) = &manifest.name {
                    api_params.push(("hostname".to_string(), name.clone()));
                }
                for (k, v) in params {
                    api_params.push((k.clone(), v.clone()));
                }

                let param_refs: Vec<(&str, &str)> = api_params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let path = format!("/nodes/{node}/lxc");
                client
                    .execute_task(&path, &param_refs, &node, 300, true, false)
                    .await?;

                if state == "running" {
                    apply_ct_state(client, next_id, &node, "running").await?;
                }

                Ok(ApplyResult {
                    vmid: Some(next_id),
                    message: format!("created + {state}"),
                })
            }
        }
    }
}

async fn apply_ct_state(
    client: &ProxmoxClient,
    vmid: u32,
    node: &str,
    desired: &str,
) -> Result<(), Error> {
    let action = match desired {
        "running" => "start",
        "stopped" => "shutdown",
        other => return Err(Error::Config(format!("invalid desired state: {other}"))),
    };
    let path = format!("/nodes/{node}/lxc/{vmid}/status/{action}");
    let params: Vec<(&str, &str)> = vec![];
    client
        .execute_task(&path, &params, node, 300, true, false)
        .await?;
    Ok(())
}
