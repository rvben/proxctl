use clap::Subcommand;
use owo_colors::OwoColorize;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::{OutputConfig, use_color};

#[derive(Subcommand)]
pub enum ServiceCommand {
    /// Start a service
    Start {
        /// Node name
        node: String,
        /// Service name
        service: String,
    },
    /// Stop a service
    Stop {
        /// Node name
        node: String,
        /// Service name
        service: String,
    },
    /// Restart a service
    Restart {
        /// Node name
        node: String,
        /// Service name
        service: String,
    },
}

#[derive(Subcommand)]
pub enum NetworkCommand {
    /// List network interfaces
    List {
        /// Node name
        node: String,
    },
    /// Show network interface details
    Show {
        /// Node name
        node: String,
        /// Interface name
        iface: String,
    },
}

#[derive(Subcommand)]
pub enum DiskCommand {
    /// List disks
    List {
        /// Node name
        node: String,
    },
    /// Show SMART data for a disk
    Smart {
        /// Node name
        node: String,
        /// Disk device path (e.g. /dev/sda)
        disk: String,
    },
}

#[derive(Subcommand)]
pub enum AptCommand {
    /// List available package updates
    List {
        /// Node name
        node: String,
    },
    /// Refresh package index
    Update {
        /// Node name
        node: String,
    },
}

#[derive(Subcommand)]
pub enum CertificateCommand {
    /// Show certificate info
    Info {
        /// Node name
        node: String,
    },
}

#[derive(Subcommand)]
pub enum NodeCommand {
    /// List all nodes
    List,
    /// Show node status
    Status {
        /// Node name (uses default node if not specified)
        node: Option<String>,
    },
    /// Shutdown a node
    Shutdown {
        /// Node name
        node: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Reboot a node
    Reboot {
        /// Node name
        node: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Start all VMs and containers on a node
    StartAll {
        /// Node name
        node: String,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Stop all VMs and containers on a node
    StopAll {
        /// Node name
        node: String,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List services on a node
    Services {
        /// Node name
        node: String,
    },
    /// Manage node services
    #[command(subcommand)]
    Service(ServiceCommand),
    /// Network interface operations
    #[command(subcommand)]
    Network(NetworkCommand),
    /// Disk operations
    #[command(subcommand)]
    Disk(DiskCommand),
    /// Show node syslog
    Syslog {
        /// Node name
        node: String,
        /// Number of log lines to show
        #[arg(long, default_value = "50")]
        lines: u64,
    },
    /// APT package management
    #[command(subcommand)]
    Apt(AptCommand),
    /// TLS certificate operations
    #[command(subcommand)]
    Certificate(CertificateCommand),
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

fn colorize_status(status: &str, width: usize) -> String {
    let padded = format!("{:<width$}", status);
    if !use_color() {
        return padded;
    }
    match status {
        "online" => padded.green().to_string(),
        "offline" => padded.red().to_string(),
        _ => padded.yellow().to_string(),
    }
}

fn require_node<'a>(
    node_arg: Option<&'a str>,
    global_node: Option<&'a str>,
) -> Result<&'a str, Error> {
    node_arg
        .or(global_node)
        .ok_or_else(|| Error::Config("node name required (use --node or PROXMOX_NODE)".to_string()))
}

fn confirm_action(action: &str, yes: bool) -> Result<(), Error> {
    if yes {
        return Ok(());
    }
    eprint!("Are you sure you want to {action}? [y/N] ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::Other(format!("failed to read input: {e}")))?;
    if input.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        Err(Error::Other("aborted".to_string()))
    }
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: NodeCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        NodeCommand::List => list(client, out).await,
        NodeCommand::Status { node } => {
            let n = require_node(node.as_deref(), global_node)?;
            status(client, out, n).await
        }
        NodeCommand::Shutdown { node, yes } => shutdown(client, out, &node, yes).await,
        NodeCommand::Reboot { node, yes } => reboot(client, out, &node, yes).await,
        NodeCommand::StartAll {
            node,
            timeout,
            r#async,
        } => start_all(client, out, &node, timeout, r#async).await,
        NodeCommand::StopAll {
            node,
            timeout,
            r#async,
            yes,
        } => stop_all(client, out, &node, timeout, r#async, yes).await,
        NodeCommand::Services { node } => services(client, out, &node).await,
        NodeCommand::Service(sub) => match sub {
            ServiceCommand::Start { node, service } => {
                service_action(client, out, &node, &service, "start").await
            }
            ServiceCommand::Stop { node, service } => {
                service_action(client, out, &node, &service, "stop").await
            }
            ServiceCommand::Restart { node, service } => {
                service_action(client, out, &node, &service, "restart").await
            }
        },
        NodeCommand::Network(sub) => match sub {
            NetworkCommand::List { node } => network_list(client, out, &node).await,
            NetworkCommand::Show { node, iface } => network_show(client, out, &node, &iface).await,
        },
        NodeCommand::Disk(sub) => match sub {
            DiskCommand::List { node } => disk_list(client, out, &node).await,
            DiskCommand::Smart { node, disk } => disk_smart(client, out, &node, &disk).await,
        },
        NodeCommand::Syslog { node, lines } => syslog(client, out, &node, lines).await,
        NodeCommand::Apt(sub) => match sub {
            AptCommand::List { node } => apt_list(client, out, &node).await,
            AptCommand::Update { node } => apt_update(client, out, &node).await,
        },
        NodeCommand::Certificate(sub) => match sub {
            CertificateCommand::Info { node } => certificate_info(client, out, &node).await,
        },
    }
}

async fn list(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let nodes = client.list_nodes().await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&nodes).expect("serialize"));
        return Ok(());
    }

    if nodes.is_empty() {
        out.print_message("No nodes found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:<15}  {:<10}  {:>6}  {:>10}  {:>10}  {:>10}",
        "NODE", "STATUS", "CPUS", "MEMORY", "DISK", "UPTIME"
    );
    let total_w = 15 + 2 + 10 + 2 + 6 + 2 + 10 + 2 + 10 + 2 + 10;
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }

    for node in &nodes {
        let hours = node.uptime / 3600;
        let days = hours / 24;
        let uptime_str = if days > 0 {
            format!("{days}d {h}h", h = hours % 24)
        } else {
            format!("{hours}h")
        };

        if color {
            println!(
                "{:<15}  {}  {:>6}  {:>10}  {:>10}  {:>10}",
                node.node.as_str().bold(),
                colorize_status(&node.status, 10),
                node.maxcpu,
                format_bytes(node.maxmem),
                format_bytes(node.maxdisk),
                uptime_str.as_str().dimmed(),
            );
        } else {
            println!(
                "{:<15}  {}  {:>6}  {:>10}  {:>10}  {:>10}",
                node.node,
                colorize_status(&node.status, 10),
                node.maxcpu,
                format_bytes(node.maxmem),
                format_bytes(node.maxdisk),
                uptime_str,
            );
        }
    }

    Ok(())
}

async fn status(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: serde_json::Value = client.get(&format!("/nodes/{node}/status")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let uptime = data
        .pointer("/uptime")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cpus = data
        .pointer("/cpuinfo/cpus")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cpu_model = data
        .pointer("/cpuinfo/model")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let total_mem = data
        .pointer("/memory/total")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let used_mem = data
        .pointer("/memory/used")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let kernel = data
        .pointer("/kversion")
        .and_then(|v| v.as_str())
        .unwrap_or("-");

    let hours = uptime / 3600;
    let days = hours / 24;
    let uptime_str = if days > 0 {
        format!("{days}d {h}h", h = hours % 24)
    } else {
        format!("{hours}h {m}m", m = (uptime % 3600) / 60)
    };

    println!("Node: {node}");
    println!("  Uptime:  {uptime_str}");
    println!("  CPUs:    {cpus} ({cpu_model})");
    println!(
        "  Memory:  {} / {} used",
        format_bytes(used_mem),
        format_bytes(total_mem)
    );
    println!("  Kernel:  {kernel}");

    Ok(())
}

async fn shutdown(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("shutdown node {node}"), yes)?;

    let _: serde_json::Value = client
        .post(&format!("/nodes/{node}/status"), &[("command", "shutdown")])
        .await?;

    out.print_result(
        &json!({"status": "shutdown initiated", "node": node}),
        &format!("Node {node} shutdown initiated"),
    );
    Ok(())
}

async fn reboot(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("reboot node {node}"), yes)?;

    let _: serde_json::Value = client
        .post(&format!("/nodes/{node}/status"), &[("command", "reboot")])
        .await?;

    out.print_result(
        &json!({"status": "reboot initiated", "node": node}),
        &format!("Node {node} reboot initiated"),
    );
    Ok(())
}

async fn start_all(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/startall");
    let result = client
        .execute_task(
            &path,
            &[],
            node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "start-all initiated", "node": node, "upid": result.upid}),
        &format!("Start-all initiated on node {node}"),
    );
    Ok(())
}

async fn stop_all(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    timeout: u64,
    async_mode: bool,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("stop all VMs/containers on node {node}"), yes)?;

    let path = format!("/nodes/{node}/stopall");
    let result = client
        .execute_task(
            &path,
            &[],
            node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "stop-all initiated", "node": node, "upid": result.upid}),
        &format!("Stop-all initiated on node {node}"),
    );
    Ok(())
}

async fn services(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/services")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No services found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!("{:<25}  {:<10}  DESCRIPTION", "SERVICE", "STATE");
    let total_w = 25 + 2 + 10 + 2 + 11; // "DESCRIPTION" is 11 chars
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for svc in &data {
        let name = svc.get("service").and_then(|v| v.as_str()).unwrap_or("-");
        let state = svc.get("state").and_then(|v| v.as_str()).unwrap_or("-");
        let desc = svc.get("desc").and_then(|v| v.as_str()).unwrap_or("-");
        let state_colored = if color {
            match state {
                "running" => state.green().to_string(),
                "stopped" => state.red().to_string(),
                _ => state.yellow().to_string(),
            }
        } else {
            state.to_string()
        };
        if color {
            println!(
                "{:<25}  {:<10}  {}",
                name.bold(),
                state_colored,
                desc.dimmed()
            );
        } else {
            println!("{:<25}  {:<10}  {}", name, state_colored, desc);
        }
    }

    Ok(())
}

async fn service_action(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    service: &str,
    action: &str,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/services/{service}/{action}");
    let _: serde_json::Value = client.post(&path, &[]).await?;

    out.print_result(
        &json!({"status": format!("{action} initiated"), "node": node, "service": service}),
        &format!("Service {service} {action} on node {node}"),
    );
    Ok(())
}

async fn network_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/network")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No network interfaces found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:<15}  {:<10}  {:<18}  {:<15}  ACTIVE",
        "IFACE", "TYPE", "ADDRESS", "GATEWAY"
    );
    let total_w = 15 + 2 + 10 + 2 + 18 + 2 + 15 + 2 + 6; // "ACTIVE" is 6 chars
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for iface in &data {
        let name = iface.get("iface").and_then(|v| v.as_str()).unwrap_or("-");
        let itype = iface.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let address = iface.get("address").and_then(|v| v.as_str()).unwrap_or("-");
        let gateway = iface.get("gateway").and_then(|v| v.as_str()).unwrap_or("-");
        let active = iface
            .get("active")
            .and_then(|v| v.as_u64())
            .map(|v| if v == 1 { "yes" } else { "no" })
            .unwrap_or("-");
        if color {
            println!(
                "{:<15}  {:<10}  {:<18}  {:<15}  {}",
                name.bold(),
                itype.to_string().dimmed(),
                address.to_string().dimmed(),
                gateway.to_string().dimmed(),
                active
            );
        } else {
            println!(
                "{:<15}  {:<10}  {:<18}  {:<15}  {}",
                name, itype, address, gateway, active
            );
        }
    }

    Ok(())
}

async fn network_show(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    iface: &str,
) -> Result<(), Error> {
    let data: serde_json::Value = client
        .get(&format!("/nodes/{node}/network/{iface}"))
        .await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    println!("Interface: {iface}");
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

async fn disk_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/disks/list")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No disks found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:<15}  {:>10}  {:<10}  {:<10}  MODEL",
        "DEVICE", "SIZE", "TYPE", "HEALTH"
    );
    let total_w = 15 + 2 + 10 + 2 + 10 + 2 + 10 + 2 + 5; // "MODEL" is 5 chars
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for disk in &data {
        let devpath = disk.get("devpath").and_then(|v| v.as_str()).unwrap_or("-");
        let size = disk.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        let dtype = disk.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let health = disk.get("health").and_then(|v| v.as_str()).unwrap_or("-");
        let model = disk.get("model").and_then(|v| v.as_str()).unwrap_or("-");
        if color {
            println!(
                "{:<15}  {:>10}  {:<10}  {:<10}  {}",
                devpath.bold(),
                format_bytes(size).dimmed(),
                dtype.to_string().dimmed(),
                health,
                model,
            );
        } else {
            println!(
                "{:<15}  {:>10}  {:<10}  {:<10}  {}",
                devpath,
                format_bytes(size),
                dtype,
                health,
                model,
            );
        }
    }

    Ok(())
}

async fn disk_smart(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    disk: &str,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/disks/smart?disk={disk}");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let health = data
        .get("health")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    println!("SMART data for {disk}");
    println!("  Health: {health}");

    if let Some(attrs) = data.get("attributes").and_then(|v| v.as_array()) {
        println!();
        let color = use_color();
        let attr_header = format!(
            "{:<5}  {:<25}  {:>6}  {:>6}  {:>10}",
            "ID", "NAME", "VALUE", "WORST", "RAW"
        );
        let attr_total_w = 5 + 2 + 25 + 2 + 6 + 2 + 6 + 2 + 10;
        if color {
            println!("{}", attr_header.bold());
            println!("{}", "-".repeat(attr_total_w).dimmed());
        } else {
            println!("{attr_header}");
            println!("{}", "-".repeat(attr_total_w));
        }
        for attr in attrs {
            let id = attr.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = attr.get("name").and_then(|v| v.as_str()).unwrap_or("-");
            let value = attr.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
            let worst = attr.get("worst").and_then(|v| v.as_u64()).unwrap_or(0);
            let raw = attr.get("raw").and_then(|v| v.as_str()).unwrap_or("-");
            if color {
                println!(
                    "{:<5}  {:<25}  {:>6}  {:>6}  {:>10}",
                    id.to_string().dimmed(),
                    name.bold(),
                    value,
                    worst.to_string().dimmed(),
                    raw.to_string().dimmed(),
                );
            } else {
                println!(
                    "{:<5}  {:<25}  {:>6}  {:>6}  {:>10}",
                    id, name, value, worst, raw
                );
            }
        }
    }

    Ok(())
}

async fn syslog(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    lines: u64,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/syslog?limit={lines}");
    let data: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    for entry in &data {
        let line = entry.get("t").and_then(|v| v.as_str()).unwrap_or("");
        println!("{line}");
    }

    Ok(())
}

async fn apt_list(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/apt/update")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No package updates available.");
        return Ok(());
    }

    let color = use_color();
    let header = format!("{:<30}  {:<20}  {:<20}", "PACKAGE", "CURRENT", "AVAILABLE");
    let total_w = 30 + 2 + 20 + 2 + 20;
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for pkg in &data {
        let name = pkg.get("Package").and_then(|v| v.as_str()).unwrap_or("-");
        let current = pkg
            .get("OldVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let available = pkg.get("Version").and_then(|v| v.as_str()).unwrap_or("-");
        if color {
            println!(
                "{:<30}  {:<20}  {:<20}",
                name.bold(),
                current.to_string().dimmed(),
                available
            );
        } else {
            println!("{:<30}  {:<20}  {:<20}", name, current, available);
        }
    }

    Ok(())
}

async fn apt_update(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let path = format!("/nodes/{node}/apt/update");
    let result = client
        .execute_task(&path, &[], node, 120, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "package index refreshed", "node": node, "upid": result.upid}),
        &format!("Package index refreshed on node {node}"),
    );
    Ok(())
}

async fn certificate_info(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client
        .get(&format!("/nodes/{node}/certificates/info"))
        .await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No certificates found.");
        return Ok(());
    }

    for cert in &data {
        let filename = cert.get("filename").and_then(|v| v.as_str()).unwrap_or("-");
        let subject = cert.get("subject").and_then(|v| v.as_str()).unwrap_or("-");
        let issuer = cert.get("issuer").and_then(|v| v.as_str()).unwrap_or("-");
        let notafter = cert
            .get("notafter")
            .and_then(|v| v.as_u64())
            .map(|ts| ts.to_string())
            .unwrap_or_else(|| "-".to_string());

        println!("Certificate: {filename}");
        println!("  Subject:  {subject}");
        println!("  Issuer:   {issuer}");
        println!("  Expires:  {notafter}");
        println!();
    }

    Ok(())
}
