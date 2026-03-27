use std::collections::HashMap;

use serde::Serialize;

use crate::api::Error;
use crate::api::client::ProxmoxClient;

use super::manifest::{Manifest, ResourceKind};

// -- Current state from API --------------------------------------------------

#[derive(Debug)]
pub struct ResourceState {
    pub vmid: Option<u32>,
    pub node: Option<String>,
    pub power_state: Option<String>,
    pub config: HashMap<String, String>,
    pub position: Option<u32>,
}

// -- What needs to change ----------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ConfigChange {
    pub key: String,
    pub old: Option<String>,
    pub new: String,
}

#[derive(Debug)]
pub enum ReconcileAction {
    NoOp,
    Create {
        params: HashMap<String, String>,
    },
    Update {
        changes: Vec<ConfigChange>,
    },
    SetState {
        from: String,
        to: String,
    },
    CreateAndSetState {
        params: HashMap<String, String>,
        state: String,
    },
}

impl ReconcileAction {
    pub fn is_noop(&self) -> bool {
        matches!(self, ReconcileAction::NoOp)
    }

    /// Label for JSON output and summary.
    pub fn action_label(&self) -> &'static str {
        match self {
            ReconcileAction::NoOp => "noop",
            ReconcileAction::Create { .. } => "create",
            ReconcileAction::Update { .. } => "update",
            ReconcileAction::SetState { .. } => "set_state",
            ReconcileAction::CreateAndSetState { .. } => "create",
        }
    }
}

// -- Result of applying ------------------------------------------------------

#[derive(Debug)]
pub struct ApplyResult {
    pub vmid: Option<u32>,
    pub message: String,
}

// -- Trait -------------------------------------------------------------------

pub trait Reconciler {
    /// Fetch current state from Proxmox. Returns None if resource doesn't exist.
    fn get_current(
        &self,
        client: &ProxmoxClient,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Option<ResourceState>, Error>> + Send;

    /// Compute what needs to change. Never fails.
    fn diff(&self, current: Option<&ResourceState>, desired: &Manifest) -> ReconcileAction;

    /// Apply the changes.
    fn apply(
        &self,
        client: &ProxmoxClient,
        action: &ReconcileAction,
        manifest: &Manifest,
        global_node: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ApplyResult, Error>> + Send;
}

// -- Registry ----------------------------------------------------------------

use super::container::ContainerReconciler;
use super::firewall::FirewallReconciler;
use super::vm::VmReconciler;

pub enum AnyReconciler {
    Vm(VmReconciler),
    Container(ContainerReconciler),
    Firewall(FirewallReconciler),
}

pub fn reconciler_for_kind(kind: &ResourceKind) -> AnyReconciler {
    match kind {
        ResourceKind::Vm => AnyReconciler::Vm(VmReconciler),
        ResourceKind::Container => AnyReconciler::Container(ContainerReconciler),
        ResourceKind::FirewallRule => AnyReconciler::Firewall(FirewallReconciler),
    }
}
