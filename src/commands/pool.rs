use clap::Subcommand;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

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
pub enum PoolCommand {
    /// List resource pools
    List,
    /// Show pool details
    Show {
        /// Pool ID
        poolid: String,
    },
    /// Create a resource pool
    Create {
        /// Pool ID
        poolid: String,
        /// Pool comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// Update a resource pool
    Update {
        /// Pool ID
        poolid: String,
        /// Pool comment
        #[arg(long)]
        comment: Option<String>,
        /// Members to add (comma-separated VMIDs or storage names)
        #[arg(long)]
        members: Option<String>,
        /// Remove members instead of adding
        #[arg(long)]
        delete: bool,
    },
    /// Delete a resource pool
    Delete {
        /// Pool ID
        poolid: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: PoolCommand,
    _global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        PoolCommand::List => list(client, out).await,
        PoolCommand::Show { poolid } => show(client, out, &poolid).await,
        PoolCommand::Create { poolid, comment } => {
            create(client, out, &poolid, comment.as_deref()).await
        }
        PoolCommand::Update {
            poolid,
            comment,
            members,
            delete,
        } => {
            update(
                client,
                out,
                &poolid,
                comment.as_deref(),
                members.as_deref(),
                delete,
            )
            .await
        }
        PoolCommand::Delete { poolid, yes } => delete(client, out, &poolid, yes).await,
    }
}

async fn list(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/pools").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No resource pools found.");
        return Ok(());
    }

    println!("{:<20}  COMMENT", "POOLID");
    for pool in &data {
        let poolid = pool.get("poolid").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = pool.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {}", poolid, comment);
    }

    Ok(())
}

async fn show(client: &ProxmoxClient, out: OutputConfig, poolid: &str) -> Result<(), Error> {
    let data: serde_json::Value = client.get(&format!("/pools/{poolid}")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    let comment = data.get("comment").and_then(|v| v.as_str()).unwrap_or("-");
    println!("Pool: {poolid}");
    println!("  Comment: {comment}");

    if let Some(members) = data.get("members").and_then(|v| v.as_array()) {
        if members.is_empty() {
            println!("  Members: (none)");
        } else {
            println!("  Members:");
            for m in members {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                let mtype = m.get("type").and_then(|v| v.as_str()).unwrap_or("-");
                let node = m.get("node").and_then(|v| v.as_str()).unwrap_or("-");
                let status = m.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                println!("    {id}  ({mtype}, node: {node}, status: {status})");
            }
        }
    }

    Ok(())
}

async fn create(
    client: &ProxmoxClient,
    out: OutputConfig,
    poolid: &str,
    comment: Option<&str>,
) -> Result<(), Error> {
    let mut params: Vec<(&str, &str)> = vec![("poolid", poolid)];
    if let Some(c) = comment {
        params.push(("comment", c));
    }
    let _: serde_json::Value = client.post("/pools", &params).await?;

    out.print_result(
        &json!({"status": "created", "poolid": poolid}),
        &format!("Pool {poolid} created"),
    );
    Ok(())
}

async fn update(
    client: &ProxmoxClient,
    out: OutputConfig,
    poolid: &str,
    comment: Option<&str>,
    members: Option<&str>,
    delete_members: bool,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(c) = comment {
        params.push(("comment".to_string(), c.to_string()));
    }
    if let Some(m) = members {
        params.push(("vms".to_string(), m.to_string()));
        if delete_members {
            params.push(("delete".to_string(), "1".to_string()));
        }
    }

    if params.is_empty() {
        return Err(Error::Config("no update parameters specified".to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/pools/{poolid}");
    let _: serde_json::Value = client.put(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "updated", "poolid": poolid}),
        &format!("Pool {poolid} updated"),
    );
    Ok(())
}

async fn delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    poolid: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete pool {poolid}"), yes)?;

    let path = format!("/pools/{poolid}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "poolid": poolid}),
        &format!("Pool {poolid} deleted"),
    );
    Ok(())
}
