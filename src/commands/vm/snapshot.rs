use std::io::IsTerminal;

use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

pub async fn list(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/snapshot");
    let snapshots: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&snapshots).expect("serialize"));
        return Ok(());
    }

    if snapshots.is_empty() {
        out.print_message(&format!("No snapshots for VM {vmid}."));
        return Ok(());
    }

    println!("{:<20}  {:<30}  {:<20}", "NAME", "DESCRIPTION", "DATE");
    for snap in &snapshots {
        let name = snap.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let desc = snap
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let snaptime = snap.get("snaptime").and_then(|v| v.as_u64()).unwrap_or(0);
        let date = if snaptime > 0 {
            chrono_from_epoch(snaptime)
        } else {
            "-".to_string()
        };
        println!("{:<20}  {:<30}  {:<20}", name, desc, date);
    }

    Ok(())
}

/// Simple epoch to date string without pulling in chrono.
fn chrono_from_epoch(epoch: u64) -> String {
    // Return the raw timestamp; a full date formatter would need chrono
    format!("{epoch}")
}

pub async fn create(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    name: &str,
    description: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/snapshot");

    let mut params: Vec<(&str, &str)> = vec![("snapname", name)];
    if let Some(desc) = description {
        params.push(("description", desc));
    }

    let result = client
        .execute_task(&path, &params, &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "created", "vmid": vmid, "snapshot": name, "upid": result.upid}),
        &format!("Snapshot '{name}' created for VM {vmid}"),
    );
    Ok(())
}

pub async fn rollback(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    name: &str,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/snapshot/{name}/rollback");

    let result = client
        .execute_task(&path, &[], &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "rolled back", "vmid": vmid, "snapshot": name, "upid": result.upid}),
        &format!("VM {vmid} rolled back to snapshot '{name}'"),
    );
    Ok(())
}

pub async fn delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    name: &str,
    yes: bool,
) -> Result<(), Error> {
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(Error::Config(
                "Use --yes to confirm destructive operations in non-interactive mode".to_string(),
            ));
        }
        let confirm = dialoguer::Confirm::new()
            .with_prompt(format!(
                "Delete snapshot '{name}' of VM {vmid}? This cannot be undone"
            ))
            .default(false)
            .interact()
            .map_err(|e| Error::Other(e.to_string()))?;
        if !confirm {
            eprintln!("Cancelled.");
            return Ok(());
        }
    }

    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/snapshot/{name}");

    let upid: String = client.delete(&path).await?;
    let _status = client
        .wait_for_task(&upid, &node, 300, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "deleted", "vmid": vmid, "snapshot": name, "upid": upid}),
        &format!("Snapshot '{name}' deleted from VM {vmid}"),
    );
    Ok(())
}
