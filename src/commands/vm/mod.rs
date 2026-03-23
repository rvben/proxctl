mod agent;
mod cloudinit;
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
        /// VM ID
        vmid: u32,
    },
    /// Create a snapshot
    Create {
        /// VM ID
        vmid: u32,
        /// Snapshot name
        name: String,
        /// Snapshot description
        #[arg(long)]
        description: Option<String>,
    },
    /// Rollback to a snapshot
    Rollback {
        /// VM ID
        vmid: u32,
        /// Snapshot name
        name: String,
    },
    /// Delete a snapshot
    Delete {
        /// VM ID
        vmid: u32,
        /// Snapshot name
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Execute a command via the QEMU guest agent
    Exec {
        /// VM ID
        vmid: u32,
        /// Command to execute
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Read a file from the VM via the guest agent
    FileRead {
        /// VM ID
        vmid: u32,
        /// File path inside the VM
        path: String,
    },
    /// Write a file to the VM via the guest agent
    FileWrite {
        /// VM ID
        vmid: u32,
        /// File path inside the VM
        path: String,
        /// Content to write
        #[arg(long)]
        content: String,
    },
    /// Show guest agent info
    Info {
        /// VM ID
        vmid: u32,
    },
}

#[derive(Subcommand)]
pub enum FirewallCommand {
    /// List VM firewall rules
    Rules {
        /// VM ID
        vmid: u32,
    },
    /// Add a firewall rule
    Add {
        /// VM ID
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
        /// VM ID
        vmid: u32,
        /// Rule position
        #[arg(long)]
        pos: u32,
    },
}

#[derive(Subcommand)]
pub enum CloudinitCommand {
    /// Show cloud-init configuration
    Show {
        /// VM ID
        vmid: u32,
    },
    /// Update cloud-init configuration
    Set {
        /// VM ID
        vmid: u32,
        /// IP address (CIDR notation or dhcp)
        #[arg(long)]
        ipconfig0: Option<String>,
        /// Nameserver
        #[arg(long)]
        nameserver: Option<String>,
        /// Search domain
        #[arg(long)]
        searchdomain: Option<String>,
        /// SSH public key
        #[arg(long)]
        sshkeys: Option<String>,
        /// User name
        #[arg(long)]
        ciuser: Option<String>,
        /// User password
        #[arg(long)]
        cipassword: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum VmCommand {
    /// List virtual machines
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
    /// Show VM status
    Status {
        /// VM ID
        vmid: u32,
    },
    /// Start a VM
    Start {
        /// VM ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Stop a VM (hard power-off)
    Stop {
        /// VM ID
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
        /// VM ID
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
    /// Reboot VM
    Reboot {
        /// VM ID
        vmid: u32,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Reset VM (hard)
    Reset {
        /// VM ID
        vmid: u32,
    },
    /// Suspend VM
    Suspend {
        /// VM ID
        vmid: u32,
        /// Suspend to disk instead of RAM
        #[arg(long)]
        todisk: bool,
    },
    /// Resume VM
    Resume {
        /// VM ID
        vmid: u32,
    },
    /// Show VM configuration
    Config {
        /// VM ID
        vmid: u32,
    },
    /// Update VM configuration
    Set {
        /// VM ID
        vmid: u32,
        /// Memory in MB
        #[arg(long)]
        memory: Option<u64>,
        /// Number of CPU cores
        #[arg(long)]
        cores: Option<u32>,
        /// VM name
        #[arg(long)]
        name: Option<String>,
        /// VM description
        #[arg(long)]
        description: Option<String>,
        /// Start at boot
        #[arg(long)]
        onboot: Option<bool>,
    },
    /// Create a new VM
    Create {
        /// VM name
        #[arg(long)]
        name: String,
        /// Memory in MB
        #[arg(long)]
        memory: u64,
        /// Number of CPU cores
        #[arg(long)]
        cores: u32,
        /// Target node
        #[arg(long)]
        node: Option<String>,
        /// OS type (e.g. l26, win10)
        #[arg(long)]
        ostype: Option<String>,
        /// Storage for VM disk
        #[arg(long)]
        storage: Option<String>,
        /// ISO image path
        #[arg(long)]
        iso: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Return UPID immediately without waiting
        #[arg(long = "async")]
        r#async: bool,
    },
    /// Destroy a VM
    Destroy {
        /// VM ID
        vmid: u32,
        /// Remove from replication and backup jobs
        #[arg(long)]
        purge: bool,
        /// Destroy unreferenced disks owned by the VM
        #[arg(long)]
        destroy_unreferenced_disks: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Clone a VM
    Clone {
        /// Source VM ID
        vmid: u32,
        /// Name for the clone
        #[arg(long)]
        name: Option<String>,
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
    /// Migrate VM to another node
    Migrate {
        /// VM ID
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
    /// Convert VM to template
    Template {
        /// VM ID
        vmid: u32,
    },
    /// Resize a VM disk
    Resize {
        /// VM ID
        vmid: u32,
        /// Disk name (e.g. scsi0, virtio0)
        #[arg(long)]
        disk: String,
        /// New size (e.g. +10G, 50G)
        #[arg(long)]
        size: String,
    },
    /// Show console connection info
    Console {
        /// VM ID
        vmid: u32,
    },
    /// VM snapshot operations
    #[command(subcommand)]
    Snapshot(SnapshotCommand),
    /// QEMU guest agent operations
    #[command(subcommand)]
    Agent(AgentCommand),
    /// VM firewall operations
    #[command(subcommand)]
    Firewall(FirewallCommand),
    /// Cloud-init configuration
    #[command(subcommand)]
    Cloudinit(CloudinitCommand),
}

/// Dispatch a VM command.
pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: VmCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        VmCommand::List { node, status, pool } => {
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
        VmCommand::Status { vmid } => lifecycle::status(client, out, vmid, global_node).await,
        VmCommand::Start {
            vmid,
            timeout,
            r#async,
        } => lifecycle::start(client, out, vmid, global_node, timeout, r#async).await,
        VmCommand::Stop {
            vmid,
            timeout,
            r#async,
        } => lifecycle::stop(client, out, vmid, global_node, timeout, r#async).await,
        VmCommand::Shutdown {
            vmid,
            timeout,
            force_stop,
            r#async,
        } => {
            lifecycle::shutdown(client, out, vmid, global_node, timeout, force_stop, r#async).await
        }
        VmCommand::Reboot {
            vmid,
            timeout,
            r#async,
        } => lifecycle::reboot(client, out, vmid, global_node, timeout, r#async).await,
        VmCommand::Reset { vmid } => lifecycle::reset(client, out, vmid, global_node).await,
        VmCommand::Suspend { vmid, todisk } => {
            lifecycle::suspend(client, out, vmid, global_node, todisk).await
        }
        VmCommand::Resume { vmid } => lifecycle::resume(client, out, vmid, global_node).await,
        VmCommand::Config { vmid } => config::show(client, out, vmid, global_node).await,
        VmCommand::Set {
            vmid,
            memory,
            cores,
            name,
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
                name,
                description,
                onboot,
            )
            .await
        }
        VmCommand::Create {
            name,
            memory,
            cores,
            node,
            ostype,
            storage,
            iso,
            timeout,
            r#async,
        } => {
            let effective_node = node.as_deref().or(global_node);
            config::create(
                client,
                out,
                &name,
                memory,
                cores,
                effective_node,
                ostype.as_deref(),
                storage.as_deref(),
                iso.as_deref(),
                timeout,
                r#async,
            )
            .await
        }
        VmCommand::Destroy {
            vmid,
            purge,
            destroy_unreferenced_disks,
            yes,
        } => {
            config::destroy(
                client,
                out,
                vmid,
                global_node,
                purge,
                destroy_unreferenced_disks,
                yes,
            )
            .await
        }
        VmCommand::Resize { vmid, disk, size } => {
            config::resize(client, out, vmid, global_node, &disk, &size).await
        }
        VmCommand::Console { vmid } => lifecycle::console(client, out, vmid, global_node).await,
        VmCommand::Clone {
            vmid,
            name,
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
                name.as_deref(),
                target_node.as_deref(),
                full,
                timeout,
                r#async,
            )
            .await
        }
        VmCommand::Migrate {
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
        VmCommand::Template { vmid } => migrate::template(client, out, vmid, global_node).await,
        VmCommand::Snapshot(sub) => match sub {
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
        VmCommand::Agent(sub) => match sub {
            AgentCommand::Exec { vmid, command } => {
                agent::exec(client, out, vmid, global_node, &command).await
            }
            AgentCommand::FileRead { vmid, path } => {
                agent::file_read(client, out, vmid, global_node, &path).await
            }
            AgentCommand::FileWrite {
                vmid,
                path,
                content,
            } => agent::file_write(client, out, vmid, global_node, &path, &content).await,
            AgentCommand::Info { vmid } => agent::info(client, out, vmid, global_node).await,
        },
        VmCommand::Firewall(sub) => match sub {
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
        VmCommand::Cloudinit(sub) => match sub {
            CloudinitCommand::Show { vmid } => {
                cloudinit::show(client, out, vmid, global_node).await
            }
            CloudinitCommand::Set {
                vmid,
                ipconfig0,
                nameserver,
                searchdomain,
                sshkeys,
                ciuser,
                cipassword,
            } => {
                cloudinit::set(
                    client,
                    out,
                    vmid,
                    global_node,
                    ipconfig0.as_deref(),
                    nameserver.as_deref(),
                    searchdomain.as_deref(),
                    sshkeys.as_deref(),
                    ciuser.as_deref(),
                    cipassword.as_deref(),
                )
                .await
            }
        },
    }
}
