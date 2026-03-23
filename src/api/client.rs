use std::collections::HashMap;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::de::DeserializeOwned;
use tokio::sync::OnceCell;

use super::Error;
use super::token::ApiToken;
use super::types::{ApiResponse, ClusterResource, NodeStatus, TaskStatus, VersionInfo};

/// Result of executing a task via the Proxmox API.
#[derive(Debug)]
pub struct TaskResult {
    pub upid: String,
    pub status: Option<TaskStatus>,
}

/// Proxmox VE API client.
pub struct ProxmoxClient {
    client: Client,
    base_url: String,
    vm_cache: OnceCell<HashMap<u32, String>>,
}

impl ProxmoxClient {
    /// Creates a new client for the given host using API token authentication.
    pub fn new(host: &str, token: ApiToken, insecure: bool) -> Result<Self, Error> {
        let base_url = normalize_base_url(host);

        let client = Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    token
                        .auth_header()
                        .parse()
                        .map_err(|e| Error::Config(format!("invalid auth header: {e}")))?,
                );
                headers
            })
            .danger_accept_invalid_certs(insecure)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url,
            vm_cache: OnceCell::new(),
        })
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api2/json{}", self.base_url, path)
    }

    // ── Low-level HTTP methods ──────────────────────────────────────

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let url = self.api_url(path);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::from_status(status.as_u16(), body));
        }
        let envelope: ApiResponse<T> = resp.json().await?;
        Ok(envelope.data)
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T, Error> {
        let url = self.api_url(path);
        let resp = self.client.post(&url).form(params).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::from_status(status.as_u16(), body));
        }
        let envelope: ApiResponse<T> = resp.json().await?;
        Ok(envelope.data)
    }

    pub async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T, Error> {
        let url = self.api_url(path);
        let resp = self.client.put(&url).form(params).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::from_status(status.as_u16(), body));
        }
        let envelope: ApiResponse<T> = resp.json().await?;
        Ok(envelope.data)
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let url = self.api_url(path);
        let resp = self.client.delete(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::from_status(status.as_u16(), body));
        }
        let envelope: ApiResponse<T> = resp.json().await?;
        Ok(envelope.data)
    }

    // ── Raw request for API passthrough ─────────────────────────────

    /// Sends a raw request and returns the response as a JSON value.
    ///
    /// If `raw_response` is true, the full JSON body is returned.
    /// Otherwise, the `data` field is unwrapped from the response envelope.
    pub async fn raw_request(
        &self,
        method: &str,
        path: &str,
        body: Option<&[(&str, &str)]>,
        raw_response: bool,
    ) -> Result<serde_json::Value, Error> {
        let url = self.api_url(path);
        let req = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => {
                let r = self.client.post(&url);
                if let Some(params) = body {
                    r.form(params)
                } else {
                    r
                }
            }
            "PUT" => {
                let r = self.client.put(&url);
                if let Some(params) = body {
                    r.form(params)
                } else {
                    r
                }
            }
            "DELETE" => self.client.delete(&url),
            other => {
                return Err(Error::Other(format!("unsupported HTTP method: {other}")));
            }
        };

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::from_status(status.as_u16(), text));
        }

        let json: serde_json::Value = resp.json().await?;
        if raw_response {
            Ok(json)
        } else {
            Ok(json.get("data").cloned().unwrap_or(serde_json::Value::Null))
        }
    }

    // ── VMID-to-node resolution ─────────────────────────────────────

    /// Resolves the node name for a given VMID by querying /cluster/resources.
    /// Results are cached for the lifetime of the client.
    pub async fn resolve_node(&self, vmid: u32) -> Result<String, Error> {
        let cache = self
            .vm_cache
            .get_or_try_init(|| async { self.build_vm_cache().await })
            .await?;

        cache
            .get(&vmid)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("VM {vmid}")))
    }

    /// Resolves the node for a VMID. If `explicit_node` is provided, returns it directly.
    pub async fn resolve_node_for_vmid(
        &self,
        vmid: u32,
        explicit_node: Option<&str>,
    ) -> Result<String, Error> {
        match explicit_node {
            Some(node) => Ok(node.to_string()),
            None => self.resolve_node(vmid).await,
        }
    }

    async fn build_vm_cache(&self) -> Result<HashMap<u32, String>, Error> {
        let resources: Vec<ClusterResource> = self.get("/cluster/resources?type=vm").await?;
        let mut map = HashMap::new();
        for r in resources {
            if let (Some(vmid), Some(node)) = (r.vmid, r.node) {
                map.insert(vmid, node);
            }
        }
        Ok(map)
    }

    // ── Task polling ────────────────────────────────────────────────

    /// Waits for a Proxmox task to complete by polling its status.
    pub async fn wait_for_task(
        &self,
        upid: &str,
        node: &str,
        timeout_secs: u64,
        show_spinner: bool,
    ) -> Result<TaskStatus, Error> {
        let path = format!("/nodes/{node}/tasks/{upid}/status");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

        let spinner = if show_spinner {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg}")
                    .expect("valid spinner template"),
            );
            pb.set_message("Waiting for task...");
            pb.enable_steady_tick(Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        loop {
            let status: TaskStatus = self.get(&path).await?;
            if !status.is_running() {
                if let Some(ref pb) = spinner {
                    pb.finish_and_clear();
                }
                if status.is_failed() {
                    let detail = status.exitstatus.as_deref().unwrap_or("unknown error");
                    return Err(Error::TaskFailed(detail.to_string()));
                }
                return Ok(status);
            }

            if tokio::time::Instant::now() >= deadline {
                if let Some(ref pb) = spinner {
                    pb.finish_and_clear();
                }
                return Err(Error::Timeout(format!(
                    "task {upid} did not complete within {timeout_secs}s"
                )));
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Posts a request that returns a UPID and optionally waits for completion.
    pub async fn execute_task(
        &self,
        path: &str,
        body: &[(&str, &str)],
        node: &str,
        timeout_secs: u64,
        wait: bool,
        show_spinner: bool,
    ) -> Result<TaskResult, Error> {
        let upid: String = self.post(path, body).await?;
        if wait {
            let status = self
                .wait_for_task(&upid, node, timeout_secs, show_spinner)
                .await?;
            Ok(TaskResult {
                upid,
                status: Some(status),
            })
        } else {
            Ok(TaskResult { upid, status: None })
        }
    }

    // ── Convenience methods ─────────────────────────────────────────

    pub async fn get_version(&self) -> Result<VersionInfo, Error> {
        self.get("/version").await
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeStatus>, Error> {
        self.get("/nodes").await
    }

    pub async fn get_cluster_resources(
        &self,
        resource_type: Option<&str>,
    ) -> Result<Vec<ClusterResource>, Error> {
        let path = match resource_type {
            Some(t) => format!("/cluster/resources?type={t}"),
            None => "/cluster/resources".to_string(),
        };
        self.get(&path).await
    }
}

/// Extracts the node name from a UPID string.
///
/// UPID format: `UPID:node:pid:pstart:starttime:type:id:user:`
pub fn parse_upid_node(upid: &str) -> Result<String, Error> {
    let parts: Vec<&str> = upid.split(':').collect();
    if parts.len() < 3 || parts[0] != "UPID" {
        return Err(Error::Other(format!("invalid UPID format: {upid}")));
    }
    Ok(parts[1].to_string())
}

fn normalize_base_url(host: &str) -> String {
    let url = host.trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_upid_node_valid() {
        let upid = "UPID:pve1:00001234:ABCDEF01:12345678:qmstart:100:root@pam:";
        let node = parse_upid_node(upid).unwrap();
        assert_eq!(node, "pve1");
    }

    #[test]
    fn parse_upid_node_invalid() {
        let result = parse_upid_node("not-a-upid");
        assert!(result.is_err());
    }

    #[test]
    fn base_url_strips_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://pve.example.com:8006/"),
            "https://pve.example.com:8006"
        );
    }

    #[test]
    fn base_url_adds_https_for_bare_host() {
        assert_eq!(
            normalize_base_url("pve.example.com:8006"),
            "https://pve.example.com:8006"
        );
    }

    #[test]
    fn base_url_preserves_http() {
        assert_eq!(
            normalize_base_url("http://pve.local:8006"),
            "http://pve.local:8006"
        );
    }
}
