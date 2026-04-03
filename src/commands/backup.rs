use clap::Subcommand;
use owo_colors::OwoColorize;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::commands::list_args::ListArgs;
use crate::output::{OutputConfig, use_color};

fn require_node<'a>(node: Option<&'a str>, global_node: Option<&'a str>) -> Result<&'a str, Error> {
    node.or(global_node)
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

#[derive(Subcommand)]
pub enum ScheduleCommand {
    /// List backup schedules
    List,
    /// Create a backup schedule
    Create {
        /// VM/container IDs (comma-separated) or "all"
        #[arg(long)]
        vmid: String,
        /// Storage target
        #[arg(long)]
        storage: String,
        /// Schedule (cron-like, e.g. "sat 02:00")
        #[arg(long)]
        schedule: String,
        /// Backup mode (snapshot, suspend, stop)
        #[arg(long, default_value = "snapshot")]
        mode: String,
    },
    /// Delete a backup schedule
    Delete {
        /// Schedule ID
        id: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum BackupCommand {
    /// List backups
    List {
        /// Filter by VM/container ID
        #[arg(long)]
        vmid: Option<u32>,
        /// Filter by storage
        #[arg(long)]
        storage: Option<String>,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Create a backup
    Create {
        /// VM/container ID
        vmid: u32,
        /// Storage target
        #[arg(long)]
        storage: Option<String>,
        /// Backup mode (snapshot, suspend, stop)
        #[arg(long, default_value = "snapshot")]
        mode: String,
        /// Compression algorithm (zstd, lzo, gzip)
        #[arg(long)]
        compress: Option<String>,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value = "600")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Restore a backup
    Restore {
        /// Target VM/container ID
        vmid: u32,
        /// Backup archive volume ID
        archive: String,
        /// Target storage for restore
        #[arg(long)]
        storage: Option<String>,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value = "600")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Manage backup schedules
    #[command(subcommand)]
    Schedule(ScheduleCommand),
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: BackupCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        BackupCommand::List {
            vmid,
            storage,
            node,
            list: list_args,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            list(client, out, n, vmid, storage.as_deref(), &list_args).await
        }
        BackupCommand::Create {
            vmid,
            storage,
            mode,
            compress,
            node,
            timeout,
            r#async,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            create(
                client,
                out,
                n,
                vmid,
                storage.as_deref(),
                &mode,
                compress.as_deref(),
                timeout,
                r#async,
            )
            .await
        }
        BackupCommand::Restore {
            vmid,
            archive,
            storage,
            node,
            timeout,
            r#async,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            restore(
                client,
                out,
                n,
                vmid,
                &archive,
                storage.as_deref(),
                timeout,
                r#async,
            )
            .await
        }
        BackupCommand::Schedule(sub) => match sub {
            ScheduleCommand::List => schedule_list(client, out).await,
            ScheduleCommand::Create {
                vmid,
                storage,
                schedule,
                mode,
            } => schedule_create(client, out, &vmid, &storage, &schedule, &mode).await,
            ScheduleCommand::Delete { id, yes } => schedule_delete(client, out, &id, yes).await,
        },
    }
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

async fn list(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    vmid_filter: Option<u32>,
    storage_filter: Option<&str>,
    list_args: &ListArgs,
) -> Result<(), Error> {
    // Find backup-capable storages
    let storages: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/storage")).await?;

    let backup_storages: Vec<&str> = storages
        .iter()
        .filter(|s| {
            let content = s.get("content").and_then(|v| v.as_str()).unwrap_or("");
            content.contains("backup")
        })
        .filter(|s| {
            if let Some(sf) = storage_filter {
                s.get("storage")
                    .and_then(|v| v.as_str())
                    .map(|n| n == sf)
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .filter_map(|s| s.get("storage").and_then(|v| v.as_str()))
        .collect();

    let mut all_backups: Vec<serde_json::Value> = Vec::new();
    for storage_name in &backup_storages {
        let path = format!("/nodes/{node}/storage/{storage_name}/content?content=backup");
        match client.get::<Vec<serde_json::Value>>(&path).await {
            Ok(items) => all_backups.extend(items),
            Err(_) => continue,
        }
    }

    // Filter by vmid
    if let Some(vid) = vmid_filter {
        let vid_str = vid.to_string();
        all_backups.retain(|b| {
            b.get("vmid")
                .and_then(|v| {
                    v.as_u64()
                        .map(|n| n.to_string())
                        .or_else(|| v.as_str().map(String::from))
                })
                .map(|v| v == vid_str)
                .unwrap_or(false)
        });
    }

    let total = all_backups.len();

    if out.json {
        let paginated: Vec<serde_json::Value> = list_args.paginate(&all_backups).to_vec();
        let paginated = list_args.filter_fields(paginated);
        let envelope = list_args.paginated_json(&paginated, total);
        out.print_data(&serde_json::to_string_pretty(&envelope).expect("serialize"));
        return Ok(());
    }

    let page = list_args.paginate(&all_backups);

    if page.is_empty() {
        out.print_message("No backups found.");
        return Ok(());
    }

    let color = use_color();
    let header = format!("{:<50}  {:>6}  {:>10}", "VOLID", "VMID", "SIZE");
    let total_w = 50 + 2 + 6 + 2 + 10;
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for b in page {
        let volid = b.get("volid").and_then(|v| v.as_str()).unwrap_or("-");
        let vmid = b
            .get("vmid")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        let size = b.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        if color {
            println!(
                "{:<50}  {:>6}  {:>10}",
                volid.bold(),
                vmid.as_str().dimmed(),
                format_bytes(size)
            );
        } else {
            println!("{:<50}  {:>6}  {:>10}", volid, vmid, format_bytes(size));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    vmid: u32,
    storage: Option<&str>,
    mode: &str,
    compress: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let vmid_str = vmid.to_string();
    let mut params: Vec<(&str, &str)> = vec![("vmid", &vmid_str), ("mode", mode)];
    if let Some(s) = storage {
        params.push(("storage", s));
    }
    if let Some(c) = compress {
        params.push(("compress", c));
    }

    let path = format!("/nodes/{node}/vzdump");
    let result = client
        .execute_task(
            &path,
            &params,
            node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "backup created", "vmid": vmid, "upid": result.upid}),
        &format!("Backup of VM/CT {vmid} completed"),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn restore(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    vmid: u32,
    archive: &str,
    storage: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let vmid_str = vmid.to_string();
    let mut params: Vec<(&str, &str)> = vec![("vmid", &vmid_str), ("archive", archive)];
    if let Some(s) = storage {
        params.push(("storage", s));
    }

    // Try qemu restore first (most common)
    let path = format!("/nodes/{node}/qemu");
    let result = client
        .execute_task(
            &path,
            &params,
            node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "restored", "vmid": vmid, "upid": result.upid}),
        &format!("VM/CT {vmid} restored from {archive}"),
    );
    Ok(())
}

async fn schedule_list(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/backup").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No backup schedules found.");
        return Ok(());
    }

    let color = use_color();
    let sched_header = format!(
        "{:<8}  {:<20}  {:<15}  {:<10}  VMID",
        "ID", "SCHEDULE", "STORAGE", "MODE"
    );
    let sched_total_w = 8 + 2 + 20 + 2 + 15 + 2 + 10 + 2 + 4; // "VMID" is 4 chars
    if color {
        println!("{}", sched_header.bold());
        println!("{}", "-".repeat(sched_total_w).dimmed());
    } else {
        println!("{sched_header}");
        println!("{}", "-".repeat(sched_total_w));
    }
    for job in &data {
        let id = job.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let schedule = job.get("schedule").and_then(|v| v.as_str()).unwrap_or("-");
        let storage_name = job.get("storage").and_then(|v| v.as_str()).unwrap_or("-");
        let mode = job.get("mode").and_then(|v| v.as_str()).unwrap_or("-");
        let vmid = job.get("vmid").and_then(|v| v.as_str()).unwrap_or("all");
        if color {
            println!(
                "{:<8}  {:<20}  {:<15}  {:<10}  {}",
                id.to_string().dimmed(),
                schedule.bold(),
                storage_name.to_string().dimmed(),
                mode,
                vmid.to_string().dimmed()
            );
        } else {
            println!(
                "{:<8}  {:<20}  {:<15}  {:<10}  {}",
                id, schedule, storage_name, mode, vmid
            );
        }
    }

    Ok(())
}

async fn schedule_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: &str,
    storage: &str,
    schedule: &str,
    mode: &str,
) -> Result<(), Error> {
    let params: Vec<(&str, &str)> = vec![
        ("vmid", vmid),
        ("storage", storage),
        ("schedule", schedule),
        ("mode", mode),
    ];

    let _: serde_json::Value = client.post("/cluster/backup", &params).await?;

    out.print_result(
        &json!({"status": "created", "schedule": schedule, "storage": storage}),
        &format!("Backup schedule created: {schedule}"),
    );
    Ok(())
}

async fn schedule_delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    id: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete backup schedule {id}"), yes)?;

    let path = format!("/cluster/backup/{id}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "id": id}),
        &format!("Backup schedule {id} deleted"),
    );
    Ok(())
}
