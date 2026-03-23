use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

pub async fn rules(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/firewall/rules");
    let rules: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&rules).expect("serialize"));
        return Ok(());
    }

    if rules.is_empty() {
        out.print_message(&format!("No firewall rules for VM {vmid}."));
        return Ok(());
    }

    println!(
        "{:<4}  {:<8}  {:<6}  {:<8}  {:<15}  {:<15}  {:<6}  COMMENT",
        "POS", "ACTION", "TYPE", "PROTO", "SOURCE", "DEST", "DPORT"
    );
    for rule in &rules {
        let pos = rule.get("pos").and_then(|v| v.as_u64()).unwrap_or(0);
        let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("-");
        let rtype = rule.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let proto = rule.get("proto").and_then(|v| v.as_str()).unwrap_or("-");
        let source = rule.get("source").and_then(|v| v.as_str()).unwrap_or("-");
        let dest = rule.get("dest").and_then(|v| v.as_str()).unwrap_or("-");
        let dport = rule.get("dport").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = rule.get("comment").and_then(|v| v.as_str()).unwrap_or("");

        println!(
            "{:<4}  {:<8}  {:<6}  {:<8}  {:<15}  {:<15}  {:<6}  {}",
            pos, action, rtype, proto, source, dest, dport, comment
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    action: &str,
    rule_type: &str,
    enable: Option<bool>,
    source: Option<&str>,
    dest: Option<&str>,
    dport: Option<&str>,
    proto: Option<&str>,
    comment: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/firewall/rules");

    let mut params: Vec<(String, String)> = vec![
        ("action".to_string(), action.to_string()),
        ("type".to_string(), rule_type.to_string()),
    ];

    if let Some(e) = enable {
        params.push(("enable".to_string(), if e { "1" } else { "0" }.to_string()));
    }
    if let Some(s) = source {
        params.push(("source".to_string(), s.to_string()));
    }
    if let Some(d) = dest {
        params.push(("dest".to_string(), d.to_string()));
    }
    if let Some(dp) = dport {
        params.push(("dport".to_string(), dp.to_string()));
    }
    if let Some(p) = proto {
        params.push(("proto".to_string(), p.to_string()));
    }
    if let Some(c) = comment {
        params.push(("comment".to_string(), c.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let _: serde_json::Value = client.post(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "added", "vmid": vmid, "action": action, "type": rule_type}),
        &format!("Firewall rule added to VM {vmid}"),
    );
    Ok(())
}

pub async fn delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    pos: u32,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/firewall/rules/{pos}");

    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "vmid": vmid, "pos": pos}),
        &format!("Firewall rule at position {pos} deleted from VM {vmid}"),
    );
    Ok(())
}
