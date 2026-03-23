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
    let path = format!("/nodes/{node}/lxc/{vmid}/snapshot");
    let snapshots: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&snapshots).expect("serialize"));
        return Ok(());
    }

    if snapshots.is_empty() {
        out.print_message(&format!("No snapshots for container {vmid}."));
        return Ok(());
    }

    println!("{:<20}  {:<20}  DESCRIPTION", "NAME", "DATE");
    for snap in &snapshots {
        let name = snap.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let desc = snap
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let snaptime = snap.get("snaptime").and_then(|v| v.as_u64()).unwrap_or(0);
        let date = format_epoch(snaptime);
        println!("{:<20}  {:<20}  {desc}", name, date);
    }

    Ok(())
}

/// Formats a Unix epoch timestamp as a human-readable UTC date string.
fn format_epoch(epoch: u64) -> String {
    if epoch == 0 {
        return "-".to_string();
    }
    let secs = epoch as i64;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let mins = (time_of_day % 3600) / 60;

    let (y, m, d) = civil_from_days(days + 719_468);
    format!("{y:04}-{m:02}-{d:02} {hours:02}:{mins:02}")
}

/// Converts a day count (with epoch offset) to (year, month, day).
/// Algorithm from Howard Hinnant's date library.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
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
    let path = format!("/nodes/{node}/lxc/{vmid}/snapshot");

    let mut params: Vec<(&str, &str)> = vec![("snapname", name)];
    if let Some(desc) = description {
        params.push(("description", desc));
    }

    let result = client
        .execute_task(&path, &params, &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "created", "vmid": vmid, "snapshot": name, "upid": result.upid}),
        &format!("Snapshot '{name}' created for container {vmid}"),
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
    let path = format!("/nodes/{node}/lxc/{vmid}/snapshot/{name}/rollback");

    let result = client
        .execute_task(&path, &[], &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "rolled back", "vmid": vmid, "snapshot": name, "upid": result.upid}),
        &format!("Container {vmid} rolled back to snapshot '{name}'"),
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
                "Delete snapshot '{name}' of container {vmid}? This cannot be undone"
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
    let path = format!("/nodes/{node}/lxc/{vmid}/snapshot/{name}");

    let upid: String = client.delete(&path).await?;
    let _status = client
        .wait_for_task(&upid, &node, 300, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "deleted", "vmid": vmid, "snapshot": name, "upid": upid}),
        &format!("Snapshot '{name}' deleted from container {vmid}"),
    );
    Ok(())
}
