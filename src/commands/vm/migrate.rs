use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

#[allow(clippy::too_many_arguments)]
pub async fn clone(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    name: Option<&str>,
    target_node: Option<&str>,
    full: bool,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;

    // Get next free VMID for the clone
    let new_vmid: u32 = client.get("/cluster/nextid").await?;

    let mut params: Vec<(String, String)> = vec![("newid".to_string(), new_vmid.to_string())];
    if let Some(n) = name {
        params.push(("name".to_string(), n.to_string()));
    }
    if let Some(tn) = target_node {
        params.push(("target".to_string(), tn.to_string()));
    }
    if full {
        params.push(("full".to_string(), "1".to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/nodes/{node}/qemu/{vmid}/clone");

    let result = client
        .execute_task(
            &path,
            &param_refs,
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({
            "status": "cloned",
            "source_vmid": vmid,
            "new_vmid": new_vmid,
            "upid": result.upid,
        }),
        &format!("VM {vmid} cloned to {new_vmid}"),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn migrate(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    target: &str,
    online: bool,
    timeout: u64,
    async_mode: bool,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/migrate");

    let mut params: Vec<(String, String)> = vec![("target".to_string(), target.to_string())];
    if online {
        params.push(("online".to_string(), "1".to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let result = client
        .execute_task(
            &path,
            &param_refs,
            &node,
            timeout,
            !async_mode,
            out.should_show_spinner(),
        )
        .await?;

    out.print_result(
        &json!({
            "status": "migrated",
            "vmid": vmid,
            "target": target,
            "upid": result.upid,
        }),
        &format!("VM {vmid} migrated to {target}"),
    );
    Ok(())
}

pub async fn template(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/template");

    let _: serde_json::Value = client.post(&path, &[]).await?;

    out.print_result(
        &json!({"status": "converted", "vmid": vmid}),
        &format!("VM {vmid} converted to template"),
    );
    Ok(())
}
