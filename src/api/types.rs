use serde::{Deserialize, Serialize};

/// Proxmox API response envelope. All responses wrap data in `{"data": ...}`.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

/// Cluster resource (from GET /cluster/resources).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterResource {
    pub id: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    pub node: Option<String>,
    pub vmid: Option<u32>,
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub maxcpu: f64,
    #[serde(default)]
    pub cpu: f64,
    #[serde(default)]
    pub maxmem: u64,
    #[serde(default)]
    pub mem: u64,
    #[serde(default)]
    pub maxdisk: u64,
    #[serde(default)]
    pub disk: u64,
    #[serde(default)]
    pub uptime: u64,
    pub pool: Option<String>,
    pub template: Option<u32>,
}

/// Task status (from GET /nodes/{node}/tasks/{upid}/status).
#[derive(Debug, Deserialize, Serialize)]
pub struct TaskStatus {
    pub status: String,
    #[serde(default)]
    pub exitstatus: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub id: Option<String>,
    pub node: Option<String>,
    pub pid: Option<u64>,
    pub starttime: Option<u64>,
    pub upid: Option<String>,
    pub user: Option<String>,
}

impl TaskStatus {
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    pub fn is_ok(&self) -> bool {
        self.status == "stopped" && self.exitstatus.as_deref() == Some("OK")
    }

    pub fn is_failed(&self) -> bool {
        self.status == "stopped" && self.exitstatus.as_deref() != Some("OK")
    }
}

/// Version info (from GET /version).
#[derive(Debug, Deserialize, Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub release: String,
    pub repoid: Option<String>,
}

/// Node status (from GET /nodes).
#[derive(Debug, Deserialize, Serialize)]
pub struct NodeStatus {
    pub node: String,
    pub status: String,
    #[serde(default)]
    pub cpu: f64,
    #[serde(default)]
    pub maxcpu: u32,
    #[serde(default)]
    pub mem: u64,
    #[serde(default)]
    pub maxmem: u64,
    #[serde(default)]
    pub disk: u64,
    #[serde(default)]
    pub maxdisk: u64,
    #[serde(default)]
    pub uptime: u64,
}

/// Task list entry (from GET /nodes/{node}/tasks or /cluster/tasks).
#[derive(Debug, Deserialize, Serialize)]
pub struct TaskListEntry {
    pub upid: String,
    pub node: String,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub id: Option<String>,
    pub user: String,
    pub status: Option<String>,
    #[serde(default)]
    pub starttime: u64,
    pub endtime: Option<u64>,
    pub exitstatus: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_is_running() {
        let ts = TaskStatus {
            status: "running".to_string(),
            exitstatus: None,
            task_type: None,
            id: None,
            node: None,
            pid: None,
            starttime: None,
            upid: None,
            user: None,
        };
        assert!(ts.is_running());
        assert!(!ts.is_ok());
        assert!(!ts.is_failed());
    }

    #[test]
    fn task_status_is_ok() {
        let ts = TaskStatus {
            status: "stopped".to_string(),
            exitstatus: Some("OK".to_string()),
            task_type: None,
            id: None,
            node: None,
            pid: None,
            starttime: None,
            upid: None,
            user: None,
        };
        assert!(!ts.is_running());
        assert!(ts.is_ok());
        assert!(!ts.is_failed());
    }

    #[test]
    fn task_status_is_failed() {
        let ts = TaskStatus {
            status: "stopped".to_string(),
            exitstatus: Some("ERRORS".to_string()),
            task_type: None,
            id: None,
            node: None,
            pid: None,
            starttime: None,
            upid: None,
            user: None,
        };
        assert!(!ts.is_running());
        assert!(!ts.is_ok());
        assert!(ts.is_failed());
    }

    #[test]
    fn task_status_stopped_without_exitstatus_is_failed() {
        let ts = TaskStatus {
            status: "stopped".to_string(),
            exitstatus: None,
            task_type: None,
            id: None,
            node: None,
            pid: None,
            starttime: None,
            upid: None,
            user: None,
        };
        assert!(ts.is_failed());
    }

    #[test]
    fn api_response_deserializes_envelope() {
        let json = r#"{"data": {"version": "8.0", "release": "1", "repoid": "abc123"}}"#;
        let resp: ApiResponse<VersionInfo> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.version, "8.0");
        assert_eq!(resp.data.release, "1");
        assert_eq!(resp.data.repoid.as_deref(), Some("abc123"));
    }

    #[test]
    fn cluster_resource_deserializes_with_defaults() {
        let json = r#"{"id": "qemu/100", "type": "qemu", "node": "pve1"}"#;
        let res: ClusterResource = serde_json::from_str(json).unwrap();
        assert_eq!(res.id, "qemu/100");
        assert_eq!(res.resource_type, "qemu");
        assert_eq!(res.node.as_deref(), Some("pve1"));
        assert_eq!(res.cpu, 0.0);
        assert_eq!(res.maxmem, 0);
    }

    #[test]
    fn node_status_deserializes_with_defaults() {
        let json = r#"{"node": "pve1", "status": "online"}"#;
        let ns: NodeStatus = serde_json::from_str(json).unwrap();
        assert_eq!(ns.node, "pve1");
        assert_eq!(ns.status, "online");
        assert_eq!(ns.cpu, 0.0);
        assert_eq!(ns.uptime, 0);
    }

    #[test]
    fn task_list_entry_deserializes() {
        let json = r#"{
            "upid": "UPID:pve1:00001234:ABCDEF01:12345678:qmstart:100:root@pam:",
            "node": "pve1",
            "type": "qmstart",
            "user": "root@pam",
            "starttime": 1700000000
        }"#;
        let entry: TaskListEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.node, "pve1");
        assert_eq!(entry.task_type.as_deref(), Some("qmstart"));
        assert_eq!(entry.user, "root@pam");
        assert_eq!(entry.starttime, 1700000000);
    }
}
