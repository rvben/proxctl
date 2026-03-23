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
    let path = format!("/nodes/{node}/qemu/{vmid}/cloudinit");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    println!("Cloud-init configuration for VM {vmid}:");
    if let Some(arr) = data.as_array() {
        for item in arr {
            let key = item.get("key").and_then(|v| v.as_str()).unwrap_or("-");
            let value = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
            println!("  {key}: {value}");
        }
    } else if let Some(obj) = data.as_object() {
        let mut keys: Vec<&String> = obj.keys().collect();
        keys.sort();
        for key in keys {
            let val = &obj[key];
            let display = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            println!("  {key}: {display}");
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
    ipconfig0: Option<&str>,
    nameserver: Option<&str>,
    searchdomain: Option<&str>,
    sshkeys: Option<&str>,
    ciuser: Option<&str>,
    cipassword: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/config");

    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = ipconfig0 {
        params.push(("ipconfig0".to_string(), v.to_string()));
    }
    if let Some(v) = nameserver {
        params.push(("nameserver".to_string(), v.to_string()));
    }
    if let Some(v) = searchdomain {
        params.push(("searchdomain".to_string(), v.to_string()));
    }
    if let Some(v) = sshkeys {
        // SSH keys need to be URL-encoded for the Proxmox API
        let encoded = urlencoding_encode(v);
        params.push(("sshkeys".to_string(), encoded));
    }
    if let Some(v) = ciuser {
        params.push(("ciuser".to_string(), v.to_string()));
    }
    if let Some(v) = cipassword {
        params.push(("cipassword".to_string(), v.to_string()));
    }

    if params.is_empty() {
        return Err(Error::Config(
            "no cloud-init options specified".to_string(),
        ));
    }

    let param_refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let _: serde_json::Value = client.put(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "updated", "vmid": vmid}),
        &format!("Cloud-init configuration updated for VM {vmid}"),
    );
    Ok(())
}

/// Simple percent-encoding for SSH keys (newlines and special chars).
fn urlencoding_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}
