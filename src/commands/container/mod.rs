mod config;
mod firewall;
mod lifecycle;
mod migrate;
mod snapshot;

use clap::Subcommand;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// List snapshots
    List {
        /// Container ID
        vmid: u32,
    },
    /// Create a snapshot
    Create {
        /// Container ID
        vmid: u32,
        /// Snapshot name
        name: String,
        /// Snapshot description
        #[arg(long)]
        description: Option<String>,
    },
    /// Rollback to a snapshot
    Rollback {
        /// Container ID
        vmid: u32,
        /// Snapshot name
        name: String,
    },
    /// Delete a snapshot
    Delete {
        /// Container ID
        vmid: u32,
        /// Snapshot name
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum FirewallCommand {
    /// List container firewall rules
    Rules {
        /// Container ID
        vmid: u32,
    },
    /// Add a firewall rule
    Add {
        /// Container ID
        vmid: u32,
        /// Rule action (ACCEPT, DROP, REJECT)
        #[arg(long)]
        action: String,
        /// Rule type (in, out, group)
        #[arg(long, rename_all = "kebab-case")]
        r#type: String,
        /// Enable the rule
        #[arg(long)]
        enable: Option<bool>,
        /// Source address
        #[arg(long)]
        source: Option<String>,
        /// Destination address
        #[arg(long)]
        dest: Option<String>,
        /// Destination port
        #[arg(long)]
        dport: Option<String>,
        /// Protocol
        #[arg(long)]
        proto: Option<String>,
        /// Comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// Delete a firewall rule
    Delete {
        /// Container ID
        vmid: u32,
        /// Rule position
        #[arg(long)]
        pos: u32,
    },
}

#[derive(Subcommand)]
pub enum ContainerCommand {
    /// List containers
    List {
        /// Filter by node
        #[arg(long)]
        node: Option<String>,
        /// Filter by status (running, stopped)
        #[arg(long)]
        status: Option<String>,
        /// Filter by pool
        #[arg(long)]
        pool: Option<String>,
    },
    /// Show container status
    Status {
        /// Container ID
        vmid: u32,
    },
    /// Start a container
    Start {
        /// Container ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Stop a container (hard power-off)
    Stop {
        /// Container ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Graceful shutdown
    Shutdown {
        /// Container ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Force stop after timeout
        #[arg(long)]
        force_stop: bool,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Reboot container
    Reboot {
        /// Container ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Suspend container
    Suspend {
        /// Container ID
        vmid: u32,
    },
    /// Resume container
    Resume {
        /// Container ID
        vmid: u32,
    },
    /// Show container configuration
    Config {
        /// Container ID
        vmid: u32,
    },
    /// Update container configuration
    Set {
        /// Container ID
        vmid: u32,
        /// Memory in MB
        #[arg(long)]
        memory: Option<u64>,
        /// Number of CPU cores
        #[arg(long)]
        cores: Option<u32>,
        /// Container hostname
        #[arg(long)]
        hostname: Option<String>,
        /// Container description
        #[arg(long)]
        description: Option<String>,
        /// Start at boot
        #[arg(long)]
        onboot: Option<bool>,
    },
    /// Create a new container
    Create {
        /// Container hostname
        #[arg(long)]
        hostname: String,
        /// OS template (e.g. local:vztmpl/debian-12-standard_12.0-1_amd64.tar.zst)
        #[arg(long)]
        ostemplate: String,
        /// Storage for rootfs (required)
        #[arg(long)]
        storage: String,
        /// Memory in MB
        #[arg(long)]
        memory: u64,
        /// Number of CPU cores
        #[arg(long)]
        cores: u32,
        /// Target node
        #[arg(long)]
        node: Option<String>,
        /// Root password
        #[arg(long)]
        password: Option<String>,
        /// Network interface config (e.g. name=eth0,bridge=vmbr0,ip=dhcp)
        #[arg(long)]
        net0: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Destroy a container
    Destroy {
        /// Container ID
        vmid: u32,
        /// Remove from replication and backup jobs
        #[arg(long)]
        purge: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Clone a container
    Clone {
        /// Source container ID
        vmid: u32,
        /// Hostname for the clone
        #[arg(long)]
        hostname: Option<String>,
        /// Target node for the clone
        #[arg(long)]
        target_node: Option<String>,
        /// Full clone (not linked)
        #[arg(long)]
        full: bool,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Migrate container to another node
    Migrate {
        /// Container ID
        vmid: u32,
        /// Target node
        #[arg(long)]
        target: String,
        /// Online migration (live)
        #[arg(long)]
        online: bool,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Convert container to template
    Template {
        /// Container ID
        vmid: u32,
    },
    /// Resize a container disk
    Resize {
        /// Container ID
        vmid: u32,
        /// Disk name (e.g. rootfs)
        #[arg(long)]
        disk: String,
        /// New size (e.g. +10G, 50G)
        #[arg(long)]
        size: String,
    },
    /// Show console connection info
    Console {
        /// Container ID
        vmid: u32,
    },
    /// Container snapshot operations
    #[command(subcommand)]
    Snapshot(SnapshotCommand),
    /// Container firewall operations
    #[command(subcommand)]
    Firewall(FirewallCommand),
}

/// Dispatch a container command.
pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: ContainerCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        ContainerCommand::List { node, status, pool } => {
            let effective_node = node.as_deref().or(global_node);
            lifecycle::list(
                client,
                out,
                effective_node,
                status.as_deref(),
                pool.as_deref(),
            )
            .await
        }
        ContainerCommand::Status { vmid } => {
            lifecycle::status(client, out, vmid, global_node).await
        }
        ContainerCommand::Start {
            vmid,
            timeout,
            r#async,
        } => lifecycle::start(client, out, vmid, global_node, timeout, r#async).await,
        ContainerCommand::Stop {
            vmid,
            timeout,
            r#async,
        } => lifecycle::stop(client, out, vmid, global_node, timeout, r#async).await,
        ContainerCommand::Shutdown {
            vmid,
            timeout,
            force_stop,
            r#async,
        } => {
            lifecycle::shutdown(client, out, vmid, global_node, timeout, force_stop, r#async).await
        }
        ContainerCommand::Reboot {
            vmid,
            timeout,
            r#async,
        } => lifecycle::reboot(client, out, vmid, global_node, timeout, r#async).await,
        ContainerCommand::Suspend { vmid } => {
            lifecycle::suspend(client, out, vmid, global_node).await
        }
        ContainerCommand::Resume { vmid } => {
            lifecycle::resume(client, out, vmid, global_node).await
        }
        ContainerCommand::Config { vmid } => config::show(client, out, vmid, global_node).await,
        ContainerCommand::Set {
            vmid,
            memory,
            cores,
            hostname,
            description,
            onboot,
        } => {
            config::set(
                client,
                out,
                vmid,
                global_node,
                memory,
                cores,
                hostname,
                description,
                onboot,
            )
            .await
        }
        ContainerCommand::Create {
            hostname,
            ostemplate,
            storage,
            memory,
            cores,
            node,
            password,
            net0,
            timeout,
            r#async,
        } => {
            let effective_node = node.as_deref().or(global_node);
            config::create(
                client,
                out,
                &hostname,
                &ostemplate,
                &storage,
                memory,
                cores,
                effective_node,
                password.as_deref(),
                net0.as_deref(),
                timeout,
                r#async,
            )
            .await
        }
        ContainerCommand::Destroy { vmid, purge, yes } => {
            config::destroy(client, out, vmid, global_node, purge, yes).await
        }
        ContainerCommand::Resize { vmid, disk, size } => {
            config::resize(client, out, vmid, global_node, &disk, &size).await
        }
        ContainerCommand::Console { vmid } => {
            lifecycle::console(client, out, vmid, global_node).await
        }
        ContainerCommand::Clone {
            vmid,
            hostname,
            target_node,
            full,
            timeout,
            r#async,
        } => {
            migrate::clone(
                client,
                out,
                vmid,
                global_node,
                hostname.as_deref(),
                target_node.as_deref(),
                full,
                timeout,
                r#async,
            )
            .await
        }
        ContainerCommand::Migrate {
            vmid,
            target,
            online,
            timeout,
            r#async,
        } => {
            migrate::migrate(
                client,
                out,
                vmid,
                global_node,
                &target,
                online,
                timeout,
                r#async,
            )
            .await
        }
        ContainerCommand::Template { vmid } => {
            migrate::template(client, out, vmid, global_node).await
        }
        ContainerCommand::Snapshot(sub) => match sub {
            SnapshotCommand::List { vmid } => snapshot::list(client, out, vmid, global_node).await,
            SnapshotCommand::Create {
                vmid,
                name,
                description,
            } => {
                snapshot::create(
                    client,
                    out,
                    vmid,
                    global_node,
                    &name,
                    description.as_deref(),
                )
                .await
            }
            SnapshotCommand::Rollback { vmid, name } => {
                snapshot::rollback(client, out, vmid, global_node, &name).await
            }
            SnapshotCommand::Delete { vmid, name, yes } => {
                snapshot::delete(client, out, vmid, global_node, &name, yes).await
            }
        },
        ContainerCommand::Firewall(sub) => match sub {
            FirewallCommand::Rules { vmid } => {
                firewall::rules(client, out, vmid, global_node).await
            }
            FirewallCommand::Add {
                vmid,
                action,
                r#type,
                enable,
                source,
                dest,
                dport,
                proto,
                comment,
            } => {
                firewall::add(
                    client,
                    out,
                    vmid,
                    global_node,
                    &action,
                    &r#type,
                    enable,
                    source.as_deref(),
                    dest.as_deref(),
                    dport.as_deref(),
                    proto.as_deref(),
                    comment.as_deref(),
                )
                .await
            }
            FirewallCommand::Delete { vmid, pos } => {
                firewall::delete(client, out, vmid, global_node, pos).await
            }
        },
    }
}
