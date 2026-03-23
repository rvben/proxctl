use clap::Subcommand;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

async fn resolve_node<'a>(
    client: &ProxmoxClient,
    node: Option<&'a str>,
    global_node: Option<&'a str>,
) -> Result<String, Error> {
    if let Some(n) = node.or(global_node) {
        return Ok(n.to_string());
    }
    // Use first node from cluster
    let nodes = client.list_nodes().await?;
    nodes
        .first()
        .map(|n| n.node.clone())
        .ok_or_else(|| Error::Config("no nodes found and --node not specified".to_string()))
}

#[derive(Subcommand)]
pub enum OsdCommand {
    /// List Ceph OSDs
    List {
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
    /// Create a Ceph OSD
    Create {
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Disk device path (e.g. /dev/sdb)
        #[arg(long)]
        dev: String,
    },
}

#[derive(Subcommand)]
pub enum CephPoolCommand {
    /// List Ceph pools
    List {
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
    /// Create a Ceph pool
    Create {
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Pool name
        #[arg(long)]
        name: String,
        /// Number of placement groups
        #[arg(long)]
        pg_num: Option<u32>,
        /// Pool size (number of replicas)
        #[arg(long)]
        size: Option<u32>,
        /// Minimum pool size
        #[arg(long)]
        min_size: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum MonCommand {
    /// List Ceph monitors
    List {
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CephCommand {
    /// Show Ceph cluster status
    Status {
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
    /// Ceph OSD operations
    #[command(subcommand)]
    Osd(OsdCommand),
    /// Ceph pool operations
    #[command(subcommand)]
    Pool(CephPoolCommand),
    /// Ceph monitor operations
    #[command(subcommand)]
    Mon(MonCommand),
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: CephCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        CephCommand::Status { node } => {
            let n = resolve_node(client, node.as_deref(), global_node).await?;
            status(client, out, &n).await
        }
        CephCommand::Osd(sub) => match sub {
            OsdCommand::List { node } => {
                let n = resolve_node(client, node.as_deref(), global_node).await?;
                osd_list(client, out, &n).await
            }
            OsdCommand::Create { node, dev } => {
                let n = resolve_node(client, node.as_deref(), global_node).await?;
                osd_create(client, out, &n, &dev).await
            }
        },
        CephCommand::Pool(sub) => match sub {
            CephPoolCommand::List { node } => {
                let n = resolve_node(client, node.as_deref(), global_node).await?;
                pool_list(client, out, &n).await
            }
            CephPoolCommand::Create {
                node,
                name,
                pg_num,
                size,
                min_size,
            } => {
                let n = resolve_node(client, node.as_deref(), global_node).await?;
                pool_create(client, out, &n, &name, pg_num, size, min_size).await
            }
        },
        CephCommand::Mon(sub) => match sub {
            MonCommand::List { node } => {
                let n = resolve_node(client, node.as_deref(), global_node).await?;
                mon_list(client, out, &n).await
            }
        },
    }
}

async fn status(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: serde_json::Value = client.get(&format!("/nodes/{node}/ceph/status")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let health = data
        .pointer("/health/status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let osds_total = data
        .pointer("/osdmap/osdmap/num_osds")
        .or_else(|| data.pointer("/osdmap/num_osds"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let osds_up = data
        .pointer("/osdmap/osdmap/num_up_osds")
        .or_else(|| data.pointer("/osdmap/num_up_osds"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mons = data
        .pointer("/monmap/num_mons")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!("Ceph Status (via node {node})");
    println!("  Health:   {health}");
    println!("  OSDs:     {osds_up}/{osds_total} up");
    println!("  Monitors: {mons}");

    if let Some(checks) = data.pointer("/health/checks").and_then(|v| v.as_object())
        && !checks.is_empty()
    {
        println!("  Health checks:");
        for (check, details) in checks {
            let severity = details
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            let summary = details
                .pointer("/summary/message")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            println!("    [{severity}] {check}: {summary}");
        }
    }

    Ok(())
}

async fn osd_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: serde_json::Value = client.get(&format!("/nodes/{node}/ceph/osd")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let root = data.get("root").and_then(|v| v.as_object());
    let children = root
        .and_then(|r| r.get("children"))
        .and_then(|v| v.as_array());

    if let Some(osds) = children {
        if osds.is_empty() {
            out.print_message("No OSDs found.");
            return Ok(());
        }

        println!(
            "{:<6}  {:<10}  {:<10}  {:<15}",
            "ID", "STATUS", "TYPE", "HOST"
        );
        for osd in osds {
            let id = osd.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let status_val = osd.get("status").and_then(|v| v.as_str()).unwrap_or("-");
            let osd_type = osd.get("type").and_then(|v| v.as_str()).unwrap_or("-");
            let host = osd.get("host").and_then(|v| v.as_str()).unwrap_or("-");
            println!(
                "{:<6}  {:<10}  {:<10}  {:<15}",
                id, status_val, osd_type, host
            );
        }
    } else {
        out.print_message("No OSD data available.");
    }

    Ok(())
}

async fn osd_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    dev: &str,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/ceph/osd");
    let result = client
        .execute_task(
            &path,
            &[("dev", dev)],
            node,
            300,
            true,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "osd created", "node": node, "dev": dev, "upid": result.upid}),
        &format!("OSD created on {dev} (node {node})"),
    );
    Ok(())
}

async fn pool_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/ceph/pools")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No Ceph pools found.");
        return Ok(());
    }

    println!(
        "{:<20}  {:>6}  {:>6}  {:>10}",
        "POOL", "SIZE", "PG_NUM", "BYTES USED"
    );
    for pool in &data {
        let name = pool
            .get("pool_name")
            .or_else(|| pool.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let size = pool.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        let pg_num = pool.get("pg_num").and_then(|v| v.as_u64()).unwrap_or(0);
        let bytes_used = pool.get("bytes_used").and_then(|v| v.as_u64()).unwrap_or(0);
        let used_str = format_bytes(bytes_used);
        println!("{:<20}  {:>6}  {:>6}  {:>10}", name, size, pg_num, used_str);
    }

    Ok(())
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

async fn pool_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    name: &str,
    pg_num: Option<u32>,
    size: Option<u32>,
    min_size: Option<u32>,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = vec![("name".to_string(), name.to_string())];
    if let Some(pg) = pg_num {
        params.push(("pg_num".to_string(), pg.to_string()));
    }
    if let Some(s) = size {
        params.push(("size".to_string(), s.to_string()));
    }
    if let Some(ms) = min_size {
        params.push(("min_size".to_string(), ms.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/nodes/{node}/ceph/pools");
    let _: serde_json::Value = client.post(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "pool created", "name": name, "node": node}),
        &format!("Ceph pool {name} created"),
    );
    Ok(())
}

async fn mon_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/ceph/mon")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No Ceph monitors found.");
        return Ok(());
    }

    println!("{:<15}  {:<20}  ADDR", "NAME", "HOST");
    for mon in &data {
        let name = mon.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let host = mon.get("host").and_then(|v| v.as_str()).unwrap_or("-");
        let addr = mon.get("addr").and_then(|v| v.as_str()).unwrap_or("-");
        println!("{:<15}  {:<20}  {}", name, host, addr);
    }

    Ok(())
}
