pub mod container;
pub mod diff;
pub mod firewall;
pub mod manifest;
pub mod reconciler;
pub mod vm;

use clap::Args;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

use manifest::{SourcedManifest, load_manifests, validate_all};
use reconciler::{AnyReconciler, Reconciler, reconciler_for_kind};

#[derive(Args)]
pub struct ApplyCommand {
    /// Manifest file or directory (repeatable)
    #[arg(short = 'f', long = "file", required = true)]
    pub files: Vec<String>,

    /// Show what would change without applying
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompts for destructive changes
    #[arg(long, short = 'y')]
    pub yes: bool,
}

pub async fn run(
    client: &ProxmoxClient,
    output: OutputConfig,
    cmd: ApplyCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    // Load and parse all manifests
    let manifests = load_manifests(&cmd.files)?;

    // Validate all manifests (fail fast)
    validate_all(&manifests)?;

    // Process each manifest
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut any_failed = false;

    for sm in &manifests {
        match process_manifest(client, &output, &cmd, sm, global_node).await {
            Ok(result) => results.push(result),
            Err((json_result, _err)) => {
                results.push(json_result);
                any_failed = true;
            }
        }
    }

    // Print JSON output
    if output.json {
        output.print_data(&serde_json::to_string_pretty(&results).expect("serialize"));
    }

    if any_failed {
        Err(Error::Other("one or more resources failed".to_string()))
    } else {
        Ok(())
    }
}

/// Process a single manifest through the get_current -> diff -> apply pipeline.
/// Returns Ok(json_result) on success, Err((json_result, error)) on failure.
async fn process_manifest(
    client: &ProxmoxClient,
    output: &OutputConfig,
    cmd: &ApplyCommand,
    sm: &SourcedManifest,
    global_node: Option<&str>,
) -> Result<serde_json::Value, (serde_json::Value, Error)> {
    let reconciler = reconciler_for_kind(&sm.manifest.kind);

    // Get current state
    let current = match get_current_state(&reconciler, client, sm, global_node).await {
        Ok(c) => c,
        Err(e) => {
            let label = sm.label();
            if output.json {
                let json = diff::result_json(
                    sm,
                    &reconciler::ReconcileAction::NoOp,
                    sm.manifest.vmid,
                    "error",
                    Some(&e.to_string()),
                );
                return Err((json, e));
            } else {
                eprintln!("ERR {label}: {e}");
                let json = diff::result_json(
                    sm,
                    &reconciler::ReconcileAction::NoOp,
                    sm.manifest.vmid,
                    "error",
                    Some(&e.to_string()),
                );
                return Err((json, e));
            }
        }
    };

    // Resolve vmid from current state for display and apply
    let resolved_vmid = current.as_ref().and_then(|c| c.vmid).or(sm.manifest.vmid);

    // Compute diff
    let action = compute_diff(&reconciler, current.as_ref(), sm);

    // Display diff
    if !output.json {
        diff::format_diff(sm, &action, resolved_vmid);
    }

    // Dry run: record and skip
    if cmd.dry_run {
        return Ok(diff::result_json(
            sm,
            &action,
            resolved_vmid,
            "planned",
            None,
        ));
    }

    // Skip noop
    if action.is_noop() {
        return Ok(diff::result_json(sm, &action, resolved_vmid, "ok", None));
    }

    // Build a manifest copy with resolved vmid for apply
    let mut apply_manifest = sm.manifest.clone();
    if apply_manifest.vmid.is_none() {
        apply_manifest.vmid = resolved_vmid;
    }

    let apply_result =
        apply_action(&reconciler, client, &action, &apply_manifest, global_node).await;

    match apply_result {
        Ok(result) => {
            let final_vmid = result.vmid.or(resolved_vmid);
            Ok(diff::result_json(sm, &action, final_vmid, "ok", None))
        }
        Err(e) => {
            if !output.json {
                eprintln!("ERR {}: {e}", sm.label());
            }
            let json = diff::result_json(sm, &action, resolved_vmid, "error", Some(&e.to_string()));
            Err((json, e))
        }
    }
}

/// Dispatch get_current to the appropriate reconciler variant.
async fn get_current_state(
    reconciler: &AnyReconciler,
    client: &ProxmoxClient,
    sm: &SourcedManifest,
    global_node: Option<&str>,
) -> Result<Option<reconciler::ResourceState>, Error> {
    match reconciler {
        AnyReconciler::Vm(r) => r.get_current(client, &sm.manifest, global_node).await,
        AnyReconciler::Container(r) => r.get_current(client, &sm.manifest, global_node).await,
        AnyReconciler::Firewall(r) => r.get_current(client, &sm.manifest, global_node).await,
    }
}

/// Dispatch diff to the appropriate reconciler variant.
fn compute_diff(
    reconciler: &AnyReconciler,
    current: Option<&reconciler::ResourceState>,
    sm: &SourcedManifest,
) -> reconciler::ReconcileAction {
    match reconciler {
        AnyReconciler::Vm(r) => r.diff(current, &sm.manifest),
        AnyReconciler::Container(r) => r.diff(current, &sm.manifest),
        AnyReconciler::Firewall(r) => r.diff(current, &sm.manifest),
    }
}

/// Dispatch apply to the appropriate reconciler variant.
async fn apply_action(
    reconciler: &AnyReconciler,
    client: &ProxmoxClient,
    action: &reconciler::ReconcileAction,
    manifest: &manifest::Manifest,
    global_node: Option<&str>,
) -> Result<reconciler::ApplyResult, Error> {
    match reconciler {
        AnyReconciler::Vm(r) => r.apply(client, action, manifest, global_node).await,
        AnyReconciler::Container(r) => r.apply(client, action, manifest, global_node).await,
        AnyReconciler::Firewall(r) => r.apply(client, action, manifest, global_node).await,
    }
}
