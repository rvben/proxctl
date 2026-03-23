use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

pub async fn exec(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    command: &[String],
) -> Result<(), Error> {
    if command.is_empty() {
        return Err(Error::Config("no command specified".to_string()));
    }

    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/agent/exec");

    let cmd_str = command.join(" ");
    let params = [("command", cmd_str.as_str())];
    let result: serde_json::Value = client.post(&path, &params).await?;

    // The exec endpoint returns a PID; we need to poll for results
    let pid = result.get("pid").and_then(|v| v.as_u64());

    if let Some(pid) = pid {
        // Poll for result
        let status_path = format!("/nodes/{node}/qemu/{vmid}/agent/exec-status?pid={pid}");

        // Poll up to 30 seconds
        for _ in 0..30 {
            let status: serde_json::Value = client.get(&status_path).await?;
            let exited = status.get("exited").and_then(|v| v.as_u64()).unwrap_or(0);

            if exited == 1 {
                if out.json {
                    out.print_data(&serde_json::to_string_pretty(&status).expect("serialize"));
                } else {
                    if let Some(stdout) = status.get("out-data").and_then(|v| v.as_str()) {
                        print!("{stdout}");
                    }
                    if let Some(stderr) = status.get("err-data").and_then(|v| v.as_str())
                        && !stderr.is_empty()
                    {
                        eprint!("{stderr}");
                    }
                }
                return Ok(());
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        return Err(Error::Timeout(format!(
            "agent exec command did not complete within 30s (pid: {pid})"
        )));
    }

    // No PID returned; display raw result
    if out.json {
        out.print_data(&serde_json::to_string_pretty(&result).expect("serialize"));
    } else {
        out.print_message(&format!("Command submitted: {}", command.join(" ")));
    }

    Ok(())
}

pub async fn file_read(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    path: &str,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let api_path = format!("/nodes/{node}/qemu/{vmid}/agent/file-read?file={path}");
    let data: serde_json::Value = client.get(&api_path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
    } else {
        let content = data
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        print!("{content}");
    }

    Ok(())
}

pub async fn file_write(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
    file_path: &str,
    content: &str,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let api_path = format!("/nodes/{node}/qemu/{vmid}/agent/file-write");
    let params = [("file", file_path), ("content", content)];
    let _: serde_json::Value = client.post(&api_path, &params).await?;

    out.print_result(
        &json!({"status": "written", "vmid": vmid, "file": file_path}),
        &format!("File written to {file_path} on VM {vmid}"),
    );
    Ok(())
}

pub async fn info(
    client: &ProxmoxClient,
    out: OutputConfig,
    vmid: u32,
    node_override: Option<&str>,
) -> Result<(), Error> {
    let node = client.resolve_node_for_vmid(vmid, node_override).await?;
    let path = format!("/nodes/{node}/qemu/{vmid}/agent/info");
    let data: serde_json::Value = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    println!("Guest agent info for VM {vmid}:");
    if let Some(obj) = data.as_object() {
        for (key, val) in obj {
            let display = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            println!("  {key}: {display}");
        }
    } else {
        println!("  {data}");
    }

    Ok(())
}
