use clap::Subcommand;
use owo_colors::OwoColorize;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::{OutputConfig, use_color};

#[derive(Subcommand)]
pub enum HaCommand {
    /// List HA resources
    Resources,
    /// Show HA status
    Status,
}

#[derive(Subcommand)]
pub enum ClusterCommand {
    /// Show cluster status
    Status,
    /// List cluster resources
    Resources {
        /// Filter by type (vm, node, storage, sdn)
        #[arg(long, rename_all = "kebab-case")]
        r#type: Option<String>,
    },
    /// Get next available VMID
    Nextid,
    /// Show cluster log
    Log {
        /// Maximum number of entries
        #[arg(long, default_value = "50")]
        max: u64,
    },
    /// Show cluster options
    Options,
    /// High availability operations
    #[command(subcommand)]
    Ha(HaCommand),
}

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

fn colorize_status(status: &str) -> String {
    if !use_color() {
        return status.to_string();
    }
    match status {
        "online" | "running" => status.green().to_string(),
        "offline" | "stopped" => status.red().to_string(),
        _ => status.yellow().to_string(),
    }
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: ClusterCommand,
    _global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        ClusterCommand::Status => status(client, out).await,
        ClusterCommand::Resources { r#type } => resources(client, out, r#type.as_deref()).await,
        ClusterCommand::Nextid => nextid(client, out).await,
        ClusterCommand::Log { max } => log(client, out, max).await,
        ClusterCommand::Options => options(client, out).await,
        ClusterCommand::Ha(sub) => match sub {
            HaCommand::Resources => ha_resources(client, out).await,
            HaCommand::Status => ha_status(client, out).await,
        },
    }
}

async fn status(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/status").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    for item in &data {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("-");

        match item_type {
            "cluster" => {
                let version = item.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
                let nodes = item.get("nodes").and_then(|v| v.as_u64()).unwrap_or(0);
                let quorate = item
                    .get("quorate")
                    .and_then(|v| v.as_u64())
                    .map(|v| v == 1)
                    .unwrap_or(false);
                println!("Cluster: {name}");
                println!("  Version:  {version}");
                println!("  Nodes:    {nodes}");
                println!("  Quorate:  {}", if quorate { "yes" } else { "no" });
                println!();
            }
            "node" => {
                let node_status = item
                    .get("online")
                    .and_then(|v| v.as_u64())
                    .map(|v| if v == 1 { "online" } else { "offline" })
                    .unwrap_or("unknown");
                let ip = item.get("ip").and_then(|v| v.as_str()).unwrap_or("-");
                println!(
                    "  Node: {:<15}  {}  ({})",
                    name,
                    colorize_status(node_status),
                    ip
                );
            }
            _ => {}
        }
    }

    Ok(())
}

async fn resources(
    client: &ProxmoxClient,
    out: OutputConfig,
    type_filter: Option<&str>,
) -> Result<(), Error> {
    let data = client.get_cluster_resources(type_filter).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No resources found.");
        return Ok(());
    }

    println!(
        "{:<30}  {:<8}  {:<10}  {:<10}  {:>10}",
        "ID", "TYPE", "NODE", "STATUS", "MEMORY"
    );
    for r in &data {
        let status_str = r.status.as_deref().unwrap_or("-");
        let node_name = r.node.as_deref().unwrap_or("-");
        println!(
            "{:<30}  {:<8}  {:<10}  {:<10}  {:>10}",
            r.id,
            r.resource_type,
            node_name,
            colorize_status(status_str),
            format_bytes(r.maxmem),
        );
    }

    Ok(())
}

async fn nextid(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let id: serde_json::Value = client.get("/cluster/nextid").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&id).expect("serialize"));
    } else {
        let id_str = match &id {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        println!("{id_str}");
    }

    Ok(())
}

async fn log(client: &ProxmoxClient, out: OutputConfig, max: u64) -> Result<(), Error> {
    let path = format!("/cluster/log?max={max}");
    let data: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No log entries found.");
        return Ok(());
    }

    for entry in &data {
        let msg = entry.get("msg").and_then(|v| v.as_str()).unwrap_or("-");
        let tag = entry.get("tag").and_then(|v| v.as_str()).unwrap_or("-");
        let node_name = entry.get("node").and_then(|v| v.as_str()).unwrap_or("-");
        println!("[{node_name}] [{tag}] {msg}");
    }

    Ok(())
}

async fn options(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: serde_json::Value = client.get("/cluster/options").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    println!("Cluster Options:");
    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            println!("  {key}: {val_str}");
        }
    }

    Ok(())
}

async fn ha_resources(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/ha/resources").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No HA resources found.");
        return Ok(());
    }

    println!(
        "{:<20}  {:<10}  {:<10}  {:<15}",
        "SID", "STATE", "GROUP", "MAX RESTART"
    );
    for r in &data {
        let sid = r.get("sid").and_then(|v| v.as_str()).unwrap_or("-");
        let state = r.get("state").and_then(|v| v.as_str()).unwrap_or("-");
        let group = r.get("group").and_then(|v| v.as_str()).unwrap_or("-");
        let max_restart = r
            .get("max_restart")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<20}  {:<10}  {:<10}  {:<15}",
            sid, state, group, max_restart
        );
    }

    Ok(())
}

async fn ha_status(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/ha/status/current").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No HA status information.");
        return Ok(());
    }

    for item in &data {
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let status_str = item
            .get("status")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("state").and_then(|v| v.as_str()))
            .unwrap_or("-");
        let node_name = item.get("node").and_then(|v| v.as_str()).unwrap_or("-");
        println!("{:<20}  {:<15}  {}", id, status_str, node_name);
    }

    Ok(())
}
