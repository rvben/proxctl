use clap::Subcommand;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

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
pub enum StorageCommand {
    /// List storage pools
    List {
        /// Filter by node
        #[arg(long)]
        node: Option<String>,
        /// Filter by storage type
        #[arg(long, rename_all = "kebab-case")]
        r#type: Option<String>,
    },
    /// Show storage status
    Status {
        /// Storage name
        storage: String,
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
    /// List storage content
    Content {
        /// Storage name
        storage: String,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Content type filter (iso, vztmpl, backup, images)
        #[arg(long)]
        content: Option<String>,
    },
    /// Upload a file to storage
    Upload {
        /// Storage name
        storage: String,
        /// Local file path
        file: String,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Content type (iso, vztmpl)
        #[arg(long, default_value = "iso")]
        content_type: String,
    },
    /// Download a file from URL to storage
    Download {
        /// Storage name
        storage: String,
        /// URL to download
        #[arg(long)]
        url: String,
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Filename for the downloaded file
        #[arg(long)]
        filename: Option<String>,
    },
    /// Create a new storage pool
    Create {
        /// Storage type (dir, nfs, lvm, zfs, ceph, etc.)
        #[arg(long, rename_all = "kebab-case")]
        r#type: String,
        /// Storage name
        #[arg(long)]
        storage: String,
        /// Path (for dir type)
        #[arg(long)]
        path: Option<String>,
        /// Content types (comma-separated: images,rootdir,vztmpl,backup,iso,snippets)
        #[arg(long)]
        content: Option<String>,
        /// NFS server
        #[arg(long)]
        server: Option<String>,
        /// NFS export path
        #[arg(long)]
        export: Option<String>,
        /// Volume group (for LVM)
        #[arg(long)]
        vgname: Option<String>,
        /// ZFS pool name
        #[arg(long)]
        pool: Option<String>,
    },
    /// Update storage configuration
    Update {
        /// Storage name
        storage: String,
        /// Content types
        #[arg(long)]
        content: Option<String>,
        /// Disable storage
        #[arg(long)]
        disable: Option<bool>,
        /// Shared storage
        #[arg(long)]
        shared: Option<bool>,
    },
    /// Delete a storage pool
    Delete {
        /// Storage name
        storage: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: StorageCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        StorageCommand::List { node, r#type } => {
            list(
                client,
                out,
                node.as_deref().or(global_node),
                r#type.as_deref(),
            )
            .await
        }
        StorageCommand::Status { storage, node } => {
            let n = require_node(node.as_deref(), global_node)?;
            status(client, out, n, &storage).await
        }
        StorageCommand::Content {
            storage,
            node,
            content,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            list_content(client, out, n, &storage, content.as_deref()).await
        }
        StorageCommand::Upload {
            storage,
            file,
            node,
            content_type,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            upload(client, out, n, &storage, &file, &content_type).await
        }
        StorageCommand::Download {
            storage,
            url,
            node,
            filename,
        } => {
            let n = require_node(node.as_deref(), global_node)?;
            download(client, out, n, &storage, &url, filename.as_deref()).await
        }
        StorageCommand::Create {
            r#type,
            storage,
            path,
            content,
            server,
            export,
            vgname,
            pool,
        } => {
            create(
                client,
                out,
                &r#type,
                &storage,
                path.as_deref(),
                content.as_deref(),
                server.as_deref(),
                export.as_deref(),
                vgname.as_deref(),
                pool.as_deref(),
            )
            .await
        }
        StorageCommand::Update {
            storage,
            content,
            disable,
            shared,
        } => update(client, out, &storage, content.as_deref(), disable, shared).await,
        StorageCommand::Delete { storage, yes } => delete(client, out, &storage, yes).await,
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
    node: Option<&str>,
    type_filter: Option<&str>,
) -> Result<(), Error> {
    let mut data: Vec<serde_json::Value> = client.get("/storage").await?;

    if let Some(tf) = type_filter {
        data.retain(|s| {
            s.get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == tf)
                .unwrap_or(false)
        });
    }

    if let Some(n) = node {
        data.retain(|s| {
            let nodes_field = s.get("nodes").and_then(|v| v.as_str()).unwrap_or("");
            nodes_field.is_empty() || nodes_field.split(',').any(|x| x.trim() == n)
        });
    }

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No storage pools found.");
        return Ok(());
    }

    println!(
        "{:<20}  {:<10}  {:<30}  {:<10}",
        "STORAGE", "TYPE", "CONTENT", "SHARED"
    );
    for s in &data {
        let name = s.get("storage").and_then(|v| v.as_str()).unwrap_or("-");
        let stype = s.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let content = s.get("content").and_then(|v| v.as_str()).unwrap_or("-");
        let shared = s
            .get("shared")
            .and_then(|v| v.as_u64())
            .map(|v| if v == 1 { "yes" } else { "no" })
            .unwrap_or("-");
        println!(
            "{:<20}  {:<10}  {:<30}  {:<10}",
            name, stype, content, shared
        );
    }

    Ok(())
}

async fn status(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    storage: &str,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/storage/{storage}/status");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    let used = data.get("used").and_then(|v| v.as_u64()).unwrap_or(0);
    let avail = data.get("avail").and_then(|v| v.as_u64()).unwrap_or(0);
    let stype = data.get("type").and_then(|v| v.as_str()).unwrap_or("-");
    let active = data
        .get("active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("-");

    println!("Storage: {storage} (node: {node})");
    println!("  Type:      {stype}");
    println!("  Active:    {active}");
    println!("  Content:   {content}");
    println!("  Total:     {}", format_bytes(total));
    println!("  Used:      {}", format_bytes(used));
    println!("  Available: {}", format_bytes(avail));

    Ok(())
}

async fn list_content(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    storage: &str,
    content_filter: Option<&str>,
) -> Result<(), Error> {
    let mut path = format!("/nodes/{node}/storage/{storage}/content");
    if let Some(ct) = content_filter {
        path = format!("{path}?content={ct}");
    }
    let data: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No content found.");
        return Ok(());
    }

    println!("{:<50}  {:<10}  {:>10}", "VOLID", "FORMAT", "SIZE");
    for item in &data {
        let volid = item.get("volid").and_then(|v| v.as_str()).unwrap_or("-");
        let format = item.get("format").and_then(|v| v.as_str()).unwrap_or("-");
        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("{:<50}  {:<10}  {:>10}", volid, format, format_bytes(size));
    }

    Ok(())
}

async fn download(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    storage: &str,
    url: &str,
    filename: Option<&str>,
) -> Result<(), Error> {
    let path = format!("/nodes/{node}/storage/{storage}/download-url");
    let mut params: Vec<(&str, &str)> = vec![("url", url), ("content", "iso")];
    if let Some(f) = filename {
        params.push(("filename", f));
    }
    let result = client
        .execute_task(&path, &params, node, 600, true, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "download started", "storage": storage, "upid": result.upid}),
        &format!("Download to {storage} completed"),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create(
    client: &ProxmoxClient,
    out: OutputConfig,
    storage_type: &str,
    storage_name: &str,
    path_opt: Option<&str>,
    content: Option<&str>,
    server: Option<&str>,
    export: Option<&str>,
    vgname: Option<&str>,
    pool: Option<&str>,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = vec![
        ("storage".to_string(), storage_name.to_string()),
        ("type".to_string(), storage_type.to_string()),
    ];
    if let Some(p) = path_opt {
        params.push(("path".to_string(), p.to_string()));
    }
    if let Some(c) = content {
        params.push(("content".to_string(), c.to_string()));
    }
    if let Some(s) = server {
        params.push(("server".to_string(), s.to_string()));
    }
    if let Some(e) = export {
        params.push(("export".to_string(), e.to_string()));
    }
    if let Some(v) = vgname {
        params.push(("vgname".to_string(), v.to_string()));
    }
    if let Some(p) = pool {
        params.push(("pool".to_string(), p.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let _: serde_json::Value = client.post("/storage", &param_refs).await?;

    out.print_result(
        &json!({"status": "created", "storage": storage_name, "type": storage_type}),
        &format!("Storage {storage_name} created"),
    );
    Ok(())
}

async fn update(
    client: &ProxmoxClient,
    out: OutputConfig,
    storage: &str,
    content: Option<&str>,
    disable: Option<bool>,
    shared: Option<bool>,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(c) = content {
        params.push(("content".to_string(), c.to_string()));
    }
    if let Some(d) = disable {
        params.push(("disable".to_string(), if d { "1" } else { "0" }.to_string()));
    }
    if let Some(s) = shared {
        params.push(("shared".to_string(), if s { "1" } else { "0" }.to_string()));
    }

    if params.is_empty() {
        return Err(Error::Config("no update parameters specified".to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/storage/{storage}");
    let _: serde_json::Value = client.put(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "updated", "storage": storage}),
        &format!("Storage {storage} updated"),
    );
    Ok(())
}

async fn upload(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    storage: &str,
    file_path: &str,
    content_type: &str,
) -> Result<(), Error> {
    let path = std::path::Path::new(file_path);
    let file_bytes = std::fs::read(path)
        .map_err(|e| Error::Other(format!("failed to read file {file_path}: {e}")))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::Config("file path has no filename".to_string()))?
        .to_string_lossy()
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name.clone())
        .mime_str("application/octet-stream")
        .map_err(|e| Error::Other(format!("invalid mime type: {e}")))?;

    let form = reqwest::multipart::Form::new()
        .text("content", content_type.to_string())
        .part("filename", part);

    let api_path = format!("/nodes/{node}/storage/{storage}/upload");
    let upid = client.upload(&api_path, form).await?;
    let _status = client
        .wait_for_task(&upid, node, 600, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "uploaded", "storage": storage, "filename": file_name, "upid": upid}),
        &format!("Uploaded {file_name} to {storage}"),
    );
    Ok(())
}

async fn delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    storage: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete storage {storage}"), yes)?;

    let path = format!("/storage/{storage}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "storage": storage}),
        &format!("Storage {storage} deleted"),
    );
    Ok(())
}
