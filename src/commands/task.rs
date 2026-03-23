use clap::Subcommand;
use owo_colors::OwoColorize;
use serde_json::json;

use crate::api::Error;
use crate::api::client::{ProxmoxClient, parse_upid_node};
use crate::output::{OutputConfig, use_color};

#[derive(Subcommand)]
pub enum TaskCommand {
    /// List recent tasks
    List {
        /// Filter by node
        #[arg(long)]
        node: Option<String>,
        /// Filter by source (vm, container)
        #[arg(long)]
        source: Option<String>,
        /// Filter by status (running, error, ok)
        #[arg(long)]
        status: Option<String>,
    },
    /// Show task status
    Status {
        /// Task UPID
        upid: String,
    },
    /// Show task log
    Log {
        /// Task UPID
        upid: String,
    },
    /// Stop a running task
    Stop {
        /// Task UPID
        upid: String,
    },
    /// Wait for a task to complete
    Wait {
        /// Task UPID
        upid: String,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: TaskCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        TaskCommand::List {
            node,
            source,
            status,
        } => {
            list(
                client,
                out,
                node.as_deref().or(global_node),
                source.as_deref(),
                status.as_deref(),
            )
            .await
        }
        TaskCommand::Status { upid } => status(client, out, &upid).await,
        TaskCommand::Log { upid } => log(client, out, &upid).await,
        TaskCommand::Stop { upid } => stop(client, out, &upid).await,
        TaskCommand::Wait { upid, timeout } => wait(client, out, &upid, timeout).await,
    }
}

async fn list(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: Option<&str>,
    source: Option<&str>,
    status_filter: Option<&str>,
) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = if let Some(n) = node {
        let mut path = format!("/nodes/{n}/tasks");
        let mut params = Vec::new();
        if let Some(s) = source {
            params.push(format!("source={s}"));
        }
        if let Some(st) = status_filter {
            // Proxmox uses "typefilter" for running vs completed
            if st == "running" {
                params.push("statusfilter=active".to_string());
            }
        }
        if !params.is_empty() {
            path = format!("{path}?{}", params.join("&"));
        }
        client.get(&path).await?
    } else {
        client.get("/cluster/tasks").await?
    };

    // Apply client-side status filter for ok/error
    let data: Vec<&serde_json::Value> = data
        .iter()
        .filter(|t| {
            if let Some(sf) = status_filter {
                match sf {
                    "ok" => t
                        .get("exitstatus")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "OK")
                        .unwrap_or(false),
                    "error" => {
                        let exit = t.get("exitstatus").and_then(|v| v.as_str());
                        exit.is_some() && exit != Some("OK")
                    }
                    "running" => t
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "running")
                        .unwrap_or(false),
                    _ => true,
                }
            } else {
                true
            }
        })
        .collect();

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No tasks found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:<10}  {:<10}  {:<15}  {:<10}  {:<20}",
        "NODE", "TYPE", "ID", "STATUS", "USER"
    );
    let total_w = 10 + 2 + 10 + 2 + 15 + 2 + 10 + 2 + 20;
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for task in &data {
        let node_name = task.get("node").and_then(|v| v.as_str()).unwrap_or("-");
        let task_type = task.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let status_str = task.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let user = task.get("user").and_then(|v| v.as_str()).unwrap_or("-");
        if color {
            println!(
                "{:<10}  {:<10}  {:<15}  {:<10}  {:<20}",
                node_name.to_string().dimmed(),
                task_type.bold(),
                id.to_string().dimmed(),
                status_str,
                user.to_string().dimmed(),
            );
        } else {
            println!(
                "{:<10}  {:<10}  {:<15}  {:<10}  {:<20}",
                node_name, task_type, id, status_str, user,
            );
        }
    }

    Ok(())
}

async fn status(client: &ProxmoxClient, out: OutputConfig, upid: &str) -> Result<(), Error> {
    let node = parse_upid_node(upid)?;
    let path = format!("/nodes/{node}/tasks/{upid}/status");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let status_str = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let exitstatus = data
        .get("exitstatus")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let task_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("-");
    let user = data.get("user").and_then(|v| v.as_str()).unwrap_or("-");

    println!("Task: {upid}");
    println!("  Status:      {status_str}");
    println!("  Exit Status: {exitstatus}");
    println!("  Type:        {task_type}");
    println!("  User:        {user}");
    println!("  Node:        {node}");

    Ok(())
}

async fn log(client: &ProxmoxClient, out: OutputConfig, upid: &str) -> Result<(), Error> {
    let node = parse_upid_node(upid)?;
    let path = format!("/nodes/{node}/tasks/{upid}/log");
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

async fn stop(client: &ProxmoxClient, out: OutputConfig, upid: &str) -> Result<(), Error> {
    let node = parse_upid_node(upid)?;
    let path = format!("/nodes/{node}/tasks/{upid}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "stopped", "upid": upid}),
        &format!("Task {upid} stopped"),
    );
    Ok(())
}

async fn wait(
    client: &ProxmoxClient,
    out: OutputConfig,
    upid: &str,
    timeout: u64,
) -> Result<(), Error> {
    let node = parse_upid_node(upid)?;
    let task_status = client
        .wait_for_task(upid, &node, timeout, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({
            "status": task_status.status,
            "exitstatus": task_status.exitstatus,
            "upid": upid,
        }),
        &format!(
            "Task completed: {}",
            task_status.exitstatus.as_deref().unwrap_or("OK")
        ),
    );
    Ok(())
}
