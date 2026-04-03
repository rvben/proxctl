use owo_colors::OwoColorize;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::commands::list_args::ListArgs;
use crate::output::{OutputConfig, use_color};

/// Format bytes as a human-readable string (e.g. "2.00 GiB").
fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else {
        format!("{:.0} MiB", b / MIB)
    }
}

/// Colorize a VM status string for terminal output.
fn colorize_status(status: &str, width: usize) -> String {
    let padded = format!("{:<width$}", status);
    if !use_color() {
        return padded;
    }
    match status {
        "running" => padded.green().to_string(),
        "stopped" => padded.red().to_string(),
        _ => padded.yellow().to_string(),
    }
}

/// Get the current status string for a VM.
async fn get_vm_status(client: &ProxmoxClient, vmid: u32, node: &str) -> Result<String, Error> {
    let path = format!("/nodes/{node}/qemu/{vmid}/status/current");
    let data: serde_json::Value = client.get(&path).await?;
    Ok(data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string())
}

pub async fn list(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: Option<&str>,
    status_filter: Option<&str>,
    pool_filter: Option<&str>,
    list_args: &ListArgs,
) -> Result<(), Error> {
    let vms: Vec<serde_json::Value> = if let Some(n) = node {
        let items: Vec<serde_json::Value> = client.get(&format!("/nodes/{n}/qemu")).await?;
        items
            .into_iter()
            .map(|mut v| {
                // Node-level listing doesn't include the node field; inject it
                if v.get("node").is_none() {
                    v.as_object_mut()
                        .map(|m| m.insert("node".to_string(), json!(n)));
                }
                v
            })
            .collect()
    } else {
        let resources = client.get_cluster_resources(Some("vm")).await?;
        resources
            .into_iter()
            .filter(|r| r.resource_type == "qemu")
            .map(|r| serde_json::to_value(r).expect("serialize cluster resource"))
            .collect()
    };

    // Apply filters
    let vms: Vec<serde_json::Value> = vms
        .into_iter()
        .filter(|v| {
            if let Some(sf) = status_filter {
                let s = v.get("status").and_then(|x| x.as_str()).unwrap_or("");
                if s != sf {
                    return false;
                }
            }
            if let Some(pf) = pool_filter {
                let p = v.get("pool").and_then(|x| x.as_str()).unwrap_or("");
                if p != pf {
                    return false;
                }
            }
            true
        })
        .collect();

    let total = vms.len();

    if out.json {
        let paginated: Vec<serde_json::Value> = list_args.paginate(&vms).to_vec();
        let paginated = list_args.filter_fields(paginated);
        let envelope = list_args.paginated_json(&paginated, total);
        out.print_data(&serde_json::to_string_pretty(&envelope).expect("serialize"));
        return Ok(());
    }

    let page = list_args.paginate(&vms);

    if page.is_empty() {
        out.print_message("No virtual machines found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:>6}  {:<20}  {:<10}  {:<10}  {:>5}  {:>10}",
        "VMID", "NAME", "STATUS", "NODE", "CPUS", "MEMORY"
    );
    let total_w = 6 + 2 + 20 + 2 + 10 + 2 + 10 + 2 + 5 + 2 + 10;
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }

    for vm in page {
        let vmid = vm.get("vmid").and_then(|v| v.as_u64()).unwrap_or(0);
        let name = vm.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let status = vm
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let node_name = vm.get("node").and_then(|v| v.as_str()).unwrap_or("-");
        let cpus = vm
            .get("maxcpu")
            .or_else(|| vm.get("cpus"))
            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
            .unwrap_or(0);
        let maxmem = vm.get("maxmem").and_then(|v| v.as_u64()).unwrap_or(0);

        if color {
            println!(
                "{:>6}  {:<20}  {}  {:<10}  {:>5}  {:>10}",
                vmid.to_string().dimmed(),
                name.bold(),
                colorize_status(status, 10),
                node_name.to_string().dimmed(),
                cpus,
                format_bytes(maxmem),
            );
        } else {
            println!(
                "{:>6}  {:<20}  {}  {:<10}  {:>5}  {:>10}",
                vmid,
                name,
                colorize_status(status, 10),
                node_name,
                cpus,
                format_bytes(maxmem),
            );
        }
    }

    Ok(())
}

pub async fn status(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/status/current");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("-");
    let cpus = data.get("cpus").and_then(|v| v.as_u64()).unwrap_or(0);
    let maxmem = data.get("maxmem").and_then(|v| v.as_u64()).unwrap_or(0);
    let uptime = data.get("uptime").and_then(|v| v.as_u64()).unwrap_or(0);
    let pid = data.get("pid").and_then(|v| v.as_u64());

    println!("VM {vmid} ({name})");
    println!("  Status:  {}", colorize_status(status, 0));
    println!("  Node:    {node}");
    println!("  CPUs:    {cpus}");
    println!("  Memory:  {}", format_bytes(maxmem));
    if uptime > 0 {
        let hours = uptime / 3600;
        let mins = (uptime % 3600) / 60;
        println!("  Uptime:  {hours}h {mins}m");
    }
    if let Some(p) = pid {
        println!("  PID:     {p}");
    }

    Ok(())
}

pub async fn start(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;

    // Idempotency: check if already running
    let current = get_vm_status(client, vmid, &node).await?;
    if current == "running" {
        out.print_result(
            &json!({"status": "already running", "vmid": vmid}),
            &format!("VM {vmid} is already running"),
        );
        return Ok(());
    }

    let path = format!("/nodes/{node}/qemu/{vmid}/status/start");
    let result = client
        .execute_task(
            &path,
            &[],
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "started", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} started"),
    );
    Ok(())
}

pub async fn stop(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;

    // Idempotency: check if already stopped
    let current = get_vm_status(client, vmid, &node).await?;
    if current == "stopped" {
        out.print_result(
            &json!({"status": "already stopped", "vmid": vmid}),
            &format!("VM {vmid} is already stopped"),
        );
        return Ok(());
    }

    let path = format!("/nodes/{node}/qemu/{vmid}/status/stop");
    let result = client
        .execute_task(
            &path,
            &[],
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "stopped", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} stopped"),
    );
    Ok(())
}

pub async fn shutdown(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    timeout: u64,
    force_stop: bool,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;

    let current = get_vm_status(client, vmid, &node).await?;
    if current == "stopped" {
        out.print_result(
            &json!({"status": "already stopped", "vmid": vmid}),
            &format!("VM {vmid} is already stopped"),
        );
        return Ok(());
    }

    let path = format!("/nodes/{node}/qemu/{vmid}/status/shutdown");
    let mut params: Vec<(&str, &str)> = Vec::new();
    let force_str;
    let timeout_str;
    if force_stop {
        force_str = "1".to_string();
        params.push(("forceStop", &force_str));
    }
    if timeout > 0 {
        timeout_str = timeout.to_string();
        params.push(("timeout", &timeout_str));
    }

    let result = client
        .execute_task(
            &path,
            &params,
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "shutdown", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} shutdown initiated"),
    );
    Ok(())
}

pub async fn reboot(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/status/reboot");
    let timeout_str = timeout.to_string();
    let params = [("timeout", timeout_str.as_str())];

    let result = client
        .execute_task(
            &path,
            &params,
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "rebooting", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} rebooting"),
    );
    Ok(())
}

pub async fn reset(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/status/reset");

    let result = client
        .execute_task(&path, &[], &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "reset", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} reset"),
    );
    Ok(())
}

pub async fn suspend(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    todisk: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/status/suspend");
    let mut params: Vec<(&str, &str)> = Vec::new();
    let todisk_str;
    if todisk {
        todisk_str = "1".to_string();
        params.push(("todisk", &todisk_str));
    }

    let result = client
        .execute_task(&path, &params, &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "suspended", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} suspended"),
    );
    Ok(())
}

pub async fn resume(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/status/resume");

    let result = client
        .execute_task(&path, &[], &node, 300, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "resumed", "vmid": vmid, "upid": result.upid}),
        &format!("VM {vmid} resumed"),
    );
    Ok(())
}

pub async fn console(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;

    let info = json!({
        "vmid": vmid,
        "node": node,
        "type": "vnc",
        "url": format!("https://{host}/?console=kvm&vmid={vmid}&node={node}",
            host = "YOUR_PROXMOX_HOST",
            vmid = vmid,
            node = node,
        ),
        "hint": "Open the Proxmox web UI or use a VNC client to connect.",
    });

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&info).expect("serialize"));
    } else {
        println!("VM {vmid} console (node: {node})");
        println!("  Type: VNC/SPICE via Proxmox web UI");
        println!("  Hint: Open https://<proxmox-host>/?console=kvm&vmid={vmid}&node={node}");
    }

    Ok(())
}
