use std::collections::{BTreeMap, HashSet};

use clap::Subcommand;
use serde::Serialize;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::commands::apply::manifest::{DesiredState, FirewallScope, ResourceKind};
use crate::output::OutputConfig;

// -- Exportable manifest (wraps Manifest for serialization control) ----------

/// A manifest struct that serializes with proper field ordering and skips None fields.
#[derive(Debug, Serialize)]
struct ExportManifest {
    kind: ResourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vmid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<DesiredState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<FirewallScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    config: BTreeMap<String, String>,
}

// -- Denylists ---------------------------------------------------------------

fn vm_denylist() -> HashSet<&'static str> {
    ["digest", "vmgenid", "lock", "pending"].into()
}

fn container_denylist() -> HashSet<&'static str> {
    ["digest", "lock", "pending"].into()
}

fn firewall_denylist() -> HashSet<&'static str> {
    ["pos", "digest"].into()
}

fn is_denied(key: &str, denylist: &HashSet<&str>, full: bool) -> bool {
    if full {
        return false;
    }
    denylist.contains(key) || key.starts_with("unused")
}

// -- Config extraction -------------------------------------------------------

/// Extract config from a JSON API response, filtering through the denylist.
/// Returns a sorted BTreeMap for deterministic YAML output.
fn extract_config(
    data: &serde_json::Value,
    denylist: &HashSet<&str>,
    full: bool,
) -> BTreeMap<String, String> {
    let mut config = BTreeMap::new();
    if let Some(obj) = data.as_object() {
        for (k, v) in obj {
            if is_denied(k, denylist, full) {
                continue;
            }
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                serde_json::Value::Null => continue,
                other => other.to_string(),
            };
            config.insert(k.clone(), val);
        }
    }
    config
}

// -- CLI structure -----------------------------------------------------------

#[derive(Subcommand)]
pub enum ExportCommand {
    /// Export virtual machine(s) as YAML
    Vm {
        /// VM ID or name (omit for --all)
        target: Option<String>,

        /// Export all VMs
        #[arg(long)]
        all: bool,

        /// Include all config keys (skip denylist)
        #[arg(long)]
        full: bool,

        /// Include power state (running/stopped)
        #[arg(long)]
        include_state: bool,
    },

    /// Export container(s) as YAML
    Container {
        /// Container ID or name (omit for --all)
        target: Option<String>,

        /// Export all containers
        #[arg(long)]
        all: bool,

        /// Include all config keys (skip denylist)
        #[arg(long)]
        full: bool,

        /// Include power state (running/stopped)
        #[arg(long)]
        include_state: bool,
    },

    /// Export firewall rules as YAML
    Firewall {
        /// Scope: cluster, node, vm, or container
        scope: String,

        /// Target: node name or VMID (required for non-cluster scopes)
        target: Option<String>,

        /// Include all config keys (skip denylist)
        #[arg(long)]
        full: bool,
    },
}

// -- Entry point -------------------------------------------------------------

pub async fn run(
    client: &ProxmoxClient,
    _output: OutputConfig,
    cmd: ExportCommand,
    global_node: Option<&str>,
    json_explicit: bool,
) -> Result<(), Error> {
    let manifests = match cmd {
        ExportCommand::Vm {
            target,
            all,
            full,
            include_state,
        } => {
            if all {
                export_all_vms(client, global_node, full, include_state).await?
            } else {
                let target = target.ok_or_else(|| {
                    Error::Config("specify a VM ID/name or use --all".to_string())
                })?;
                vec![export_vm(client, &target, global_node, full, include_state).await?]
            }
        }
        ExportCommand::Container {
            target,
            all,
            full,
            include_state,
        } => {
            if all {
                export_all_containers(client, global_node, full, include_state).await?
            } else {
                let target = target.ok_or_else(|| {
                    Error::Config("specify a container ID/name or use --all".to_string())
                })?;
                vec![export_container(client, &target, global_node, full, include_state).await?]
            }
        }
        ExportCommand::Firewall {
            scope,
            target,
            full,
        } => export_firewall(client, &scope, target.as_deref(), global_node, full).await?,
    };

    if manifests.is_empty() {
        return Ok(());
    }

    if json_explicit {
        println!(
            "{}",
            serde_json::to_string_pretty(&manifests).expect("serialize")
        );
    } else {
        for (i, m) in manifests.iter().enumerate() {
            if i > 0 {
                println!("---");
            }
            println!(
                "{}",
                serde_yaml::to_string(m).expect("serialize").trim_end()
            );
        }
    }

    Ok(())
}

// -- VM export ---------------------------------------------------------------

/// Resolve a target string as VMID (numeric) or name (non-numeric).
fn parse_target(target: &str) -> Result<Either, Error> {
    match target.parse::<u32>() {
        Ok(vmid) => Ok(Either::Vmid(vmid)),
        Err(_) => Ok(Either::Name(target.to_string())),
    }
}

enum Either {
    Vmid(u32),
    Name(String),
}

/// Find a VM/container by name in cluster resources. Returns (vmid, node, name).
async fn find_by_name(
    client: &ProxmoxClient,
    name: &str,
    resource_type: &str,
) -> Result<(u32, String, Option<String>), Error> {
    let resources = client.get_cluster_resources(Some("vm")).await?;
    let matches: Vec<_> = resources
        .iter()
        .filter(|r| r.resource_type == resource_type && r.name.as_deref() == Some(name))
        .collect();

    match matches.len() {
        0 => Err(Error::NotFound(format!(
            "no {resource_type} named '{name}'"
        ))),
        1 => {
            let r = &matches[0];
            Ok((
                r.vmid
                    .ok_or_else(|| Error::Other("resource missing vmid".to_string()))?,
                r.node
                    .clone()
                    .ok_or_else(|| Error::Other("resource missing node".to_string()))?,
                r.name.clone(),
            ))
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
                "multiple {resource_type}s named '{name}': {}. Use VMID instead.",
                ids.join(", ")
            )))
        }
    }
}

async fn export_vm(
    client: &ProxmoxClient,
    target: &str,
    global_node: Option<&str>,
    full: bool,
    include_state: bool,
) -> Result<ExportManifest, Error> {
    let (vmid, node, name) = match parse_target(target)? {
        Either::Vmid(vmid) => {
            let node = client.resolve_node_for_vmid(vmid, global_node).await?;
            let resources = client.get_cluster_resources(Some("vm")).await?;
            let name = resources
                .iter()
                .find(|r| r.vmid == Some(vmid) && r.resource_type == "qemu")
                .and_then(|r| r.name.clone());
            (vmid, node, name)
        }
        Either::Name(ref name) => find_by_name(client, name, "qemu").await?,
    };

    let path = format!("/nodes/{node}/qemu/{vmid}/config");
    let data: serde_json::Value = client.get(&path).await?;
    let config = extract_config(&data, &vm_denylist(), full);

    let state = if include_state {
        let status_path = format!("/nodes/{node}/qemu/{vmid}/status/current");
        let status: serde_json::Value = client.get(&status_path).await?;
        match status.get("status").and_then(|v| v.as_str()) {
            Some("running") => Some(DesiredState::Running),
            Some("stopped") => Some(DesiredState::Stopped),
            _ => None,
        }
    } else {
        None
    };

    Ok(ExportManifest {
        kind: ResourceKind::Vm,
        name,
        vmid: Some(vmid),
        node: Some(node),
        state,
        scope: None,
        target: None,
        config,
    })
}

async fn export_all_vms(
    client: &ProxmoxClient,
    _global_node: Option<&str>,
    full: bool,
    include_state: bool,
) -> Result<Vec<ExportManifest>, Error> {
    let resources = client.get_cluster_resources(Some("vm")).await?;
    let mut vms: Vec<_> = resources
        .iter()
        .filter(|r| r.resource_type == "qemu" && r.template != Some(1))
        .collect();
    vms.sort_by_key(|r| r.vmid.unwrap_or(0));

    let mut manifests = Vec::new();
    for r in vms {
        let vmid = match r.vmid {
            Some(v) => v,
            None => continue,
        };
        let node = match &r.node {
            Some(n) => n.clone(),
            None => continue,
        };

        let path = format!("/nodes/{node}/qemu/{vmid}/config");
        let data: serde_json::Value = client.get(&path).await?;
        let config = extract_config(&data, &vm_denylist(), full);

        let state = if include_state {
            match r.status.as_deref() {
                Some("running") => Some(DesiredState::Running),
                Some("stopped") => Some(DesiredState::Stopped),
                _ => None,
            }
        } else {
            None
        };

        manifests.push(ExportManifest {
            kind: ResourceKind::Vm,
            name: r.name.clone(),
            vmid: Some(vmid),
            node: Some(node),
            state,
            scope: None,
            target: None,
            config,
        });
    }

    Ok(manifests)
}

// -- Container export --------------------------------------------------------

async fn export_container(
    client: &ProxmoxClient,
    target: &str,
    global_node: Option<&str>,
    full: bool,
    include_state: bool,
) -> Result<ExportManifest, Error> {
    let (vmid, node, name) = match parse_target(target)? {
        Either::Vmid(vmid) => {
            let node = client.resolve_node_for_vmid(vmid, global_node).await?;
            let resources = client.get_cluster_resources(Some("vm")).await?;
            let name = resources
                .iter()
                .find(|r| r.vmid == Some(vmid) && r.resource_type == "lxc")
                .and_then(|r| r.name.clone());
            (vmid, node, name)
        }
        Either::Name(ref name) => find_by_name(client, name, "lxc").await?,
    };

    let path = format!("/nodes/{node}/lxc/{vmid}/config");
    let data: serde_json::Value = client.get(&path).await?;
    let config = extract_config(&data, &container_denylist(), full);

    let state = if include_state {
        let status_path = format!("/nodes/{node}/lxc/{vmid}/status/current");
        let status: serde_json::Value = client.get(&status_path).await?;
        match status.get("status").and_then(|v| v.as_str()) {
            Some("running") => Some(DesiredState::Running),
            Some("stopped") => Some(DesiredState::Stopped),
            _ => None,
        }
    } else {
        None
    };

    Ok(ExportManifest {
        kind: ResourceKind::Container,
        name,
        vmid: Some(vmid),
        node: Some(node),
        state,
        scope: None,
        target: None,
        config,
    })
}

async fn export_all_containers(
    client: &ProxmoxClient,
    _global_node: Option<&str>,
    full: bool,
    include_state: bool,
) -> Result<Vec<ExportManifest>, Error> {
    let resources = client.get_cluster_resources(Some("vm")).await?;
    let mut cts: Vec<_> = resources
        .iter()
        .filter(|r| r.resource_type == "lxc")
        .collect();
    cts.sort_by_key(|r| r.vmid.unwrap_or(0));

    let mut manifests = Vec::new();
    for r in cts {
        let vmid = match r.vmid {
            Some(v) => v,
            None => continue,
        };
        let node = match &r.node {
            Some(n) => n.clone(),
            None => continue,
        };

        let path = format!("/nodes/{node}/lxc/{vmid}/config");
        let data: serde_json::Value = client.get(&path).await?;
        let config = extract_config(&data, &container_denylist(), full);

        let state = if include_state {
            match r.status.as_deref() {
                Some("running") => Some(DesiredState::Running),
                Some("stopped") => Some(DesiredState::Stopped),
                _ => None,
            }
        } else {
            None
        };

        manifests.push(ExportManifest {
            kind: ResourceKind::Container,
            name: r.name.clone(),
            vmid: Some(vmid),
            node: Some(node),
            state,
            scope: None,
            target: None,
            config,
        });
    }

    Ok(manifests)
}

// -- Firewall export ---------------------------------------------------------

async fn export_firewall(
    client: &ProxmoxClient,
    scope_str: &str,
    target: Option<&str>,
    global_node: Option<&str>,
    full: bool,
) -> Result<Vec<ExportManifest>, Error> {
    let (scope, api_path, export_target) = match scope_str {
        "cluster" => (
            FirewallScope::Cluster,
            "/cluster/firewall/rules".to_string(),
            None,
        ),
        "node" => {
            let node = target
                .ok_or_else(|| Error::Config("node scope requires a node name".to_string()))?;
            (
                FirewallScope::Node,
                format!("/nodes/{node}/firewall/rules"),
                Some(node.to_string()),
            )
        }
        "vm" => {
            let vmid_str =
                target.ok_or_else(|| Error::Config("vm scope requires a VMID".to_string()))?;
            let vmid: u32 = vmid_str
                .parse()
                .map_err(|_| Error::Config(format!("invalid VMID: {vmid_str}")))?;
            let node = client.resolve_node_for_vmid(vmid, global_node).await?;
            (
                FirewallScope::Vm,
                format!("/nodes/{node}/qemu/{vmid}/firewall/rules"),
                Some(vmid_str.to_string()),
            )
        }
        "container" => {
            let vmid_str = target
                .ok_or_else(|| Error::Config("container scope requires a VMID".to_string()))?;
            let vmid: u32 = vmid_str
                .parse()
                .map_err(|_| Error::Config(format!("invalid VMID: {vmid_str}")))?;
            let node = client.resolve_node_for_vmid(vmid, global_node).await?;
            (
                FirewallScope::Container,
                format!("/nodes/{node}/lxc/{vmid}/firewall/rules"),
                Some(vmid_str.to_string()),
            )
        }
        other => {
            return Err(Error::Config(format!(
                "invalid firewall scope: '{other}'. Use: cluster, node, vm, container"
            )));
        }
    };

    let rules: Vec<serde_json::Value> = client.get(&api_path).await?;
    let denylist = firewall_denylist();

    let mut manifests = Vec::new();
    for rule in &rules {
        let config = extract_config(rule, &denylist, full);
        manifests.push(ExportManifest {
            kind: ResourceKind::FirewallRule,
            name: None,
            vmid: None,
            node: None,
            state: None,
            scope: Some(scope.clone()),
            target: export_target.clone(),
            config,
        });
    }

    Ok(manifests)
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylist_filters_digest() {
        let denylist = vm_denylist();
        assert!(is_denied("digest", &denylist, false));
        assert!(!is_denied("digest", &denylist, true));
    }

    #[test]
    fn denylist_filters_unused_prefix() {
        let denylist = vm_denylist();
        assert!(is_denied("unused0", &denylist, false));
        assert!(is_denied("unused123", &denylist, false));
        assert!(!is_denied("unused0", &denylist, true));
    }

    #[test]
    fn denylist_allows_normal_keys() {
        let denylist = vm_denylist();
        assert!(!is_denied("memory", &denylist, false));
        assert!(!is_denied("cores", &denylist, false));
        assert!(!is_denied("scsi0", &denylist, false));
        assert!(!is_denied("net0", &denylist, false));
    }

    #[test]
    fn extract_config_filters_and_sorts() {
        let data = serde_json::json!({
            "memory": 4096,
            "cores": 2,
            "digest": "abc123",
            "name": "test",
            "unused0": "local-lvm:vm-100-disk-1",
        });
        let denylist = vm_denylist();
        let config = extract_config(&data, &denylist, false);

        assert_eq!(config.get("memory").unwrap(), "4096");
        assert_eq!(config.get("cores").unwrap(), "2");
        assert_eq!(config.get("name").unwrap(), "test");
        assert!(!config.contains_key("digest"));
        assert!(!config.contains_key("unused0"));

        // BTreeMap is sorted
        let keys: Vec<&String> = config.keys().collect();
        assert_eq!(keys, vec!["cores", "memory", "name"]);
    }

    #[test]
    fn extract_config_full_skips_denylist() {
        let data = serde_json::json!({
            "memory": 4096,
            "digest": "abc123",
            "unused0": "disk",
        });
        let denylist = vm_denylist();
        let config = extract_config(&data, &denylist, true);

        assert!(config.contains_key("digest"));
        assert!(config.contains_key("unused0"));
    }

    #[test]
    fn extract_config_converts_types() {
        let data = serde_json::json!({
            "memory": 4096,
            "onboot": true,
            "description": "test vm",
        });
        let config = extract_config(&data, &HashSet::new(), false);

        assert_eq!(config.get("memory").unwrap(), "4096");
        assert_eq!(config.get("onboot").unwrap(), "1");
        assert_eq!(config.get("description").unwrap(), "test vm");
    }

    #[test]
    fn extract_config_skips_null() {
        let data = serde_json::json!({
            "memory": 4096,
            "lock": null,
        });
        let config = extract_config(&data, &HashSet::new(), false);
        assert!(!config.contains_key("lock"));
    }

    #[test]
    fn parse_target_numeric() {
        match parse_target("101").unwrap() {
            Either::Vmid(v) => assert_eq!(v, 101),
            _ => panic!("expected Vmid"),
        }
    }

    #[test]
    fn parse_target_name() {
        match parse_target("haos").unwrap() {
            Either::Name(n) => assert_eq!(n, "haos"),
            _ => panic!("expected Name"),
        }
    }

    #[test]
    fn container_denylist_has_no_vmgenid() {
        let denylist = container_denylist();
        assert!(!denylist.contains("vmgenid"));
        assert!(denylist.contains("digest"));
    }

    #[test]
    fn firewall_denylist_filters_pos() {
        let denylist = firewall_denylist();
        assert!(is_denied("pos", &denylist, false));
        assert!(!is_denied("action", &denylist, false));
    }

    #[test]
    fn export_manifest_serializes_to_yaml() {
        let m = ExportManifest {
            kind: ResourceKind::Vm,
            name: Some("test".to_string()),
            vmid: Some(100),
            node: Some("pve1".to_string()),
            state: None,
            scope: None,
            target: None,
            config: [
                ("memory".to_string(), "4096".to_string()),
                ("cores".to_string(), "2".to_string()),
            ]
            .into(),
        };
        let yaml = serde_yaml::to_string(&m).unwrap();
        assert!(yaml.contains("kind: vm"));
        assert!(yaml.contains("name: test"));
        assert!(yaml.contains("vmid: 100"));
        assert!(yaml.contains("node: pve1"));
        assert!(!yaml.contains("scope"));
        assert!(!yaml.contains("target"));
        assert!(!yaml.contains("state"));
    }

    #[test]
    fn export_manifest_firewall_serializes_scope() {
        let m = ExportManifest {
            kind: ResourceKind::FirewallRule,
            name: None,
            vmid: None,
            node: None,
            state: None,
            scope: Some(FirewallScope::Cluster),
            target: None,
            config: [("action".to_string(), "ACCEPT".to_string())].into(),
        };
        let yaml = serde_yaml::to_string(&m).unwrap();
        assert!(yaml.contains("kind: firewall-rule"));
        assert!(yaml.contains("scope: cluster"));
        assert!(!yaml.contains("name"));
        assert!(!yaml.contains("vmid"));
    }

    #[test]
    fn export_manifest_with_state() {
        let m = ExportManifest {
            kind: ResourceKind::Vm,
            name: Some("test".to_string()),
            vmid: Some(100),
            node: Some("pve1".to_string()),
            state: Some(DesiredState::Running),
            scope: None,
            target: None,
            config: BTreeMap::new(),
        };
        let yaml = serde_yaml::to_string(&m).unwrap();
        assert!(yaml.contains("state: running"));
    }
}
