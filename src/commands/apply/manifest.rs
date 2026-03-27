use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::api::Error;

// -- Types -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Vm,
    Container,
    FirewallRule,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesiredState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FirewallScope {
    Cluster,
    Node,
    Vm,
    Container,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub kind: ResourceKind,
    pub name: Option<String>,
    pub vmid: Option<u32>,
    pub node: Option<String>,
    pub state: Option<DesiredState>,
    pub scope: Option<FirewallScope>,
    pub target: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
}

// -- Source tracking ---------------------------------------------------------

/// A manifest with its source file and document index for error reporting.
#[derive(Debug)]
pub struct SourcedManifest {
    pub manifest: Manifest,
    pub file: String,
    pub doc_index: usize,
}

impl SourcedManifest {
    /// Human-readable label like "vm/web-01" or "firewall-rule/Allow HTTPS".
    pub fn label(&self) -> String {
        let kind = match self.manifest.kind {
            ResourceKind::Vm => "vm",
            ResourceKind::Container => "container",
            ResourceKind::FirewallRule => "firewall-rule",
        };
        let name = self
            .manifest
            .name
            .as_deref()
            .or_else(|| self.manifest.config.get("comment").and_then(|v| v.as_str()))
            .unwrap_or("unnamed");
        format!("{kind}/{name}")
    }
}

// -- Parsing -----------------------------------------------------------------

/// Parse a YAML string (potentially multi-document) into manifests.
pub fn parse_yaml(content: &str, file: &str) -> Result<Vec<SourcedManifest>, Error> {
    let mut manifests = Vec::new();
    for (i, doc) in serde_yaml::Deserializer::from_str(content).enumerate() {
        let manifest: Manifest = Manifest::deserialize(doc)
            .map_err(|e| Error::Config(format!("{file} (doc {}): {e}", i + 1)))?;
        manifests.push(SourcedManifest {
            manifest,
            file: file.to_string(),
            doc_index: i + 1,
        });
    }
    if manifests.is_empty() {
        return Err(Error::Config(format!("{file}: no YAML documents found")));
    }
    Ok(manifests)
}

/// Load manifests from a file path or directory.
/// Directories are scanned for *.yaml and *.yml files (non-recursive).
pub fn load_manifests(paths: &[String]) -> Result<Vec<SourcedManifest>, Error> {
    let mut all = Vec::new();
    for path_str in paths {
        let path = Path::new(path_str);
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .map_err(|e| Error::Config(format!("cannot read directory {path_str}: {e}")))?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.ends_with(".yaml") || name.ends_with(".yml")
                })
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let file_path = entry.path();
                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    Error::Config(format!("cannot read {}: {e}", file_path.display()))
                })?;
                all.extend(parse_yaml(&content, &file_path.to_string_lossy())?);
            }
        } else {
            let content = std::fs::read_to_string(path)
                .map_err(|e| Error::Config(format!("cannot read {path_str}: {e}")))?;
            all.extend(parse_yaml(&content, path_str)?);
        }
    }
    if all.is_empty() {
        return Err(Error::Config("no manifests found".to_string()));
    }
    Ok(all)
}

// -- Validation --------------------------------------------------------------

/// Validate a manifest, returning a list of error messages (empty = valid).
pub fn validate(sm: &SourcedManifest) -> Vec<String> {
    let m = &sm.manifest;
    let prefix = format!("{} (doc {}) {}", sm.file, sm.doc_index, sm.label());
    let mut errors = Vec::new();

    match m.kind {
        ResourceKind::Vm | ResourceKind::Container => {
            if m.name.is_none() && m.vmid.is_none() {
                errors.push(format!("{prefix}: must specify 'name' or 'vmid' (or both)"));
            }
        }
        ResourceKind::FirewallRule => {
            if m.scope.is_none() {
                errors.push(format!("{prefix}: missing required field 'scope'"));
            }
            if let Some(scope) = &m.scope {
                match scope {
                    FirewallScope::Node | FirewallScope::Vm | FirewallScope::Container => {
                        if m.target.is_none() {
                            errors.push(format!(
                                "{prefix}: 'target' is required for scope '{}'",
                                match scope {
                                    FirewallScope::Node => "node",
                                    FirewallScope::Vm => "vm",
                                    FirewallScope::Container => "container",
                                    _ => unreachable!(),
                                }
                            ));
                        }
                    }
                    FirewallScope::Cluster => {}
                }
            }
            if !m.config.contains_key("action") {
                errors.push(format!("{prefix}: missing required config key 'action'"));
            }
            if !m.config.contains_key("type") {
                errors.push(format!("{prefix}: missing required config key 'type'"));
            }
        }
    }

    if m.config.is_empty() {
        errors.push(format!("{prefix}: 'config' must not be empty"));
    }

    errors
}

/// Validate all manifests. Returns Ok(()) or Err with all validation errors.
pub fn validate_all(manifests: &[SourcedManifest]) -> Result<(), Error> {
    let errors: Vec<String> = manifests.iter().flat_map(validate).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        let msg = format!("Validation errors:\n  {}", errors.join("\n  "));
        Err(Error::Config(msg))
    }
}

// -- Helpers -----------------------------------------------------------------

/// Convert a serde_yaml::Value to a String for API parameters.
pub fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_vm() {
        let yaml = r#"
kind: vm
name: web-01
vmid: 100
state: running
config:
  memory: 4096
  cores: 2
"#;
        let manifests = parse_yaml(yaml, "test.yaml").unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].manifest.kind, ResourceKind::Vm);
        assert_eq!(manifests[0].manifest.name.as_deref(), Some("web-01"));
        assert_eq!(manifests[0].manifest.vmid, Some(100));
        assert_eq!(manifests[0].manifest.state, Some(DesiredState::Running));
        assert_eq!(manifests[0].manifest.config.len(), 2);
    }

    #[test]
    fn parse_multi_document() {
        let yaml = r#"
kind: vm
name: web-01
config:
  memory: 4096
---
kind: container
name: dns-01
config:
  memory: 512
"#;
        let manifests = parse_yaml(yaml, "test.yaml").unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(manifests[0].manifest.kind, ResourceKind::Vm);
        assert_eq!(manifests[1].manifest.kind, ResourceKind::Container);
    }

    #[test]
    fn parse_firewall_rule() {
        let yaml = r#"
kind: firewall-rule
scope: cluster
config:
  action: ACCEPT
  type: in
  source: 10.0.0.0/24
  comment: "Allow internal"
"#;
        let manifests = parse_yaml(yaml, "test.yaml").unwrap();
        assert_eq!(manifests[0].manifest.kind, ResourceKind::FirewallRule);
        assert_eq!(manifests[0].manifest.scope, Some(FirewallScope::Cluster));
    }

    #[test]
    fn parse_empty_yaml_errors() {
        let result = parse_yaml("", "empty.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_yaml_errors() {
        let result = parse_yaml("kind: bogus\nconfig:\n  x: 1\n", "bad.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn validate_vm_requires_name_or_vmid() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::Vm,
                name: None,
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: [("memory".to_string(), serde_yaml::Value::Number(4096.into()))].into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        let errors = validate(&sm);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("name"));
    }

    #[test]
    fn validate_vm_with_name_passes() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::Vm,
                name: Some("web-01".to_string()),
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: [("memory".to_string(), serde_yaml::Value::Number(4096.into()))].into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        assert!(validate(&sm).is_empty());
    }

    #[test]
    fn validate_firewall_requires_scope() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::FirewallRule,
                name: None,
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: [
                    (
                        "action".to_string(),
                        serde_yaml::Value::String("ACCEPT".to_string()),
                    ),
                    (
                        "type".to_string(),
                        serde_yaml::Value::String("in".to_string()),
                    ),
                ]
                .into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        let errors = validate(&sm);
        assert!(errors.iter().any(|e| e.contains("scope")));
    }

    #[test]
    fn validate_firewall_node_scope_requires_target() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::FirewallRule,
                name: None,
                vmid: None,
                node: None,
                state: None,
                scope: Some(FirewallScope::Node),
                target: None,
                config: [
                    (
                        "action".to_string(),
                        serde_yaml::Value::String("ACCEPT".to_string()),
                    ),
                    (
                        "type".to_string(),
                        serde_yaml::Value::String("in".to_string()),
                    ),
                ]
                .into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        let errors = validate(&sm);
        assert!(errors.iter().any(|e| e.contains("target")));
    }

    #[test]
    fn validate_firewall_requires_action_and_type() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::FirewallRule,
                name: None,
                vmid: None,
                node: None,
                state: None,
                scope: Some(FirewallScope::Cluster),
                target: None,
                config: [(
                    "comment".to_string(),
                    serde_yaml::Value::String("test".to_string()),
                )]
                .into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        let errors = validate(&sm);
        assert!(errors.iter().any(|e| e.contains("action")));
        assert!(errors.iter().any(|e| e.contains("type")));
    }

    #[test]
    fn validate_empty_config_errors() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::Vm,
                name: Some("test".to_string()),
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: HashMap::new(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        let errors = validate(&sm);
        assert!(errors.iter().any(|e| e.contains("config")));
    }

    #[test]
    fn yaml_bool_converts_to_proxmox_int() {
        assert_eq!(yaml_value_to_string(&serde_yaml::Value::Bool(true)), "1");
        assert_eq!(yaml_value_to_string(&serde_yaml::Value::Bool(false)), "0");
    }

    #[test]
    fn label_uses_name() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::Vm,
                name: Some("web-01".to_string()),
                vmid: None,
                node: None,
                state: None,
                scope: None,
                target: None,
                config: HashMap::new(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        assert_eq!(sm.label(), "vm/web-01");
    }

    #[test]
    fn label_firewall_uses_comment() {
        let sm = SourcedManifest {
            manifest: Manifest {
                kind: ResourceKind::FirewallRule,
                name: None,
                vmid: None,
                node: None,
                state: None,
                scope: Some(FirewallScope::Cluster),
                target: None,
                config: [(
                    "comment".to_string(),
                    serde_yaml::Value::String("Allow HTTPS".to_string()),
                )]
                .into(),
            },
            file: "test.yaml".to_string(),
            doc_index: 1,
        };
        assert_eq!(sm.label(), "firewall-rule/Allow HTTPS");
    }

    #[test]
    fn load_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.yaml"),
            "kind: vm\nname: a\nconfig:\n  memory: 1024\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.yml"),
            "kind: vm\nname: b\nconfig:\n  memory: 2048\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("c.txt"), "not yaml").unwrap();

        let manifests = load_manifests(&[dir.path().to_string_lossy().to_string()]).unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(manifests[0].manifest.name.as_deref(), Some("a"));
        assert_eq!(manifests[1].manifest.name.as_deref(), Some("b"));
    }
}
