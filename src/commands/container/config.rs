use std::io::IsTerminal;

use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

pub async fn show(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/lxc/{vmid}/config");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if let Some(obj) = data.as_object() {
        let mut keys: Vec<&String> = obj.keys().collect();
        keys.sort();
        let max_key_len = keys.iter().map(|k| k.len()).max().unwrap_or(0);
        let col_width = max_key_len.max(3) + 2;

        println!("{:<col_width$}  VALUE", "KEY");
        for key in keys {
            let val = &obj[key];
            let display = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            println!("{:<col_width$}  {display}", key);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn set(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    memory: Option<u64>,
    cores: Option<u32>,
    hostname: Option<String>,
    description: Option<String>,
    onboot: Option<bool>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/lxc/{vmid}/config");

    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(m) = memory {
        params.push(("memory".to_string(), m.to_string()));
    }
    if let Some(c) = cores {
        params.push(("cores".to_string(), c.to_string()));
    }
    if let Some(h) = hostname {
        params.push(("hostname".to_string(), h));
    }
    if let Some(d) = description {
        params.push(("description".to_string(), d));
    }
    if let Some(ob) = onboot {
        params.push(("onboot".to_string(), if ob { "1" } else { "0" }.to_string()));
    }

    if params.is_empty() {
        return Err(Error::Config(
            "no configuration options specified".to_string(),
        ));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let _: serde_json::Value = client.put(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "updated", "vmid": vmid}),
        &format!("Container {vmid} configuration updated"),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    client: &ProxmoxClient,
    out: OutputConfig,
    hostname: &str,
    ostemplate: &str,
    storage: &str,
    memory: u64,
    cores: u32,
    node: Option<&str>,
    password: Option<&str>,
    net0: Option<&str>,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let target_node = match node {
        Some(n) => n.to_string(),
        None => {
            let nodes = client.list_nodes().await?;
            nodes
                .first()
                .map(|n| n.node.clone())
                .ok_or_else(|| Error::Config("no nodes available".to_string()))?
        }
    };

    let next_id: u32 = client.get("/cluster/nextid").await?;

    let mut params: Vec<(String, String)> = vec![
        ("vmid".to_string(), next_id.to_string()),
        ("hostname".to_string(), hostname.to_string()),
        ("ostemplate".to_string(), ostemplate.to_string()),
        ("storage".to_string(), storage.to_string()),
        ("memory".to_string(), memory.to_string()),
        ("cores".to_string(), cores.to_string()),
    ];

    if let Some(pw) = password {
        params.push(("password".to_string(), pw.to_string()));
    }
    if let Some(n) = net0 {
        params.push(("net0".to_string(), n.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/nodes/{target_node}/lxc");

    let result = client
        .execute_task(
            &path,
            &param_refs,
            &target_node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({"status": "created", "vmid": next_id, "node": target_node, "upid": result.upid}),
        &format!("Container {next_id} ({hostname}) created on {target_node}"),
    );
    Ok(())
}

pub async fn destroy(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    purge: bool,
    yes: bool,
) -> Result<(), Error> {
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(Error::Config(
                "Use --yes to confirm destructive operations in non-interactive mode".to_string(),
            ));
        }
        let confirm = dialoguer::Confirm::new()
            .with_prompt(format!("Destroy container {vmid}? This cannot be undone"))
            .default(false)
            .interact()
            .map_err(|e| Error::Other(e.to_string()))?;
        if !confirm {
            eprintln!("Cancelled.");
            return Ok(());
        }
    }

    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let mut path = format!("/nodes/{node}/lxc/{vmid}");

    if purge {
        path = format!("{path}?purge=1");
    }

    let upid: String = client.delete(&path).await?;
    let _status = client
        .wait_for_task(&upid, &node, 300, out.should_show_spinner())
        .await?;

    out.print_result(
        &json!({"status": "destroyed", "vmid": vmid, "upid": upid}),
        &format!("Container {vmid} destroyed"),
    );
    Ok(())
}

pub async fn resize(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    disk: &str,
    size: &str,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/lxc/{vmid}/resize");

    let params = [("disk", disk), ("size", size)];
    let _: serde_json::Value = client.put(&path, &params).await?;

    out.print_result(
        &json!({"status": "resized", "vmid": vmid, "disk": disk, "size": size}),
        &format!("Container {vmid} disk {disk} resized to {size}"),
    );
    Ok(())
}
