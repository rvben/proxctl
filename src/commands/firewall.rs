use clap::{Args, Subcommand};
use owo_colors::OwoColorize;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::{OutputConfig, use_color};

#[derive(Args)]
pub struct FirewallRuleArgs {
    /// Rule action (ACCEPT, DROP, REJECT)
    #[arg(long)]
    pub action: String,
    /// Rule type (in, out, group)
    #[arg(long, rename_all = "kebab-case")]
    pub r#type: String,
    /// Enable the rule
    #[arg(long)]
    pub enable: Option<bool>,
    /// Network interface (e.g. vmbr0, vmbr0v30)
    #[arg(long)]
    pub iface: Option<String>,
    /// Source address
    #[arg(long)]
    pub source: Option<String>,
    /// Destination address
    #[arg(long)]
    pub dest: Option<String>,
    /// Destination port
    #[arg(long)]
    pub dport: Option<String>,
    /// Protocol
    #[arg(long)]
    pub proto: Option<String>,
    /// Macro (e.g. SSH, HTTP, HTTPS)
    #[arg(long, rename_all = "kebab-case")]
    pub r#macro: Option<String>,
    /// Comment
    #[arg(long)]
    pub comment: Option<String>,
}

fn require_node<'a>(node: Option<&'a str>, global_node: Option<&'a str>) -> Result<&'a str, Error> {
    node.or(global_node)
        .ok_or_else(|| Error::Config("node name required (use --node or PROXMOX_NODE)".to_string()))
}

fn confirm_action(action: &str, yes: bool) -> Result<(), Error> {
    if yes {
        return Ok(());
    }
    eprint!("Are you sure you want to {action}? [y/N] ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::Other(format!("failed to read input: {e}")))?;
    if input.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        Err(Error::Other("aborted".to_string()))
    }
}

#[derive(Subcommand)]
pub enum ClusterFirewallCommand {
    /// List cluster firewall rules
    Rules,
    /// Add a cluster firewall rule
    Add {
        #[command(flatten)]
        rule: Box<FirewallRuleArgs>,
    },
    /// Delete a cluster firewall rule
    Delete {
        /// Rule position
        #[arg(long)]
        pos: u32,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum NodeFirewallCommand {
    /// List node firewall rules
    Rules {
        /// Node name
        #[arg(long)]
        node: Option<String>,
    },
    /// Add a node firewall rule
    Add {
        /// Node name
        #[arg(long)]
        node: Option<String>,
        #[command(flatten)]
        rule: Box<FirewallRuleArgs>,
    },
    /// Delete a node firewall rule
    Delete {
        /// Node name
        #[arg(long)]
        node: Option<String>,
        /// Rule position
        #[arg(long)]
        pos: u32,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum GroupCommand {
    /// Show security group rules
    Show {
        /// Group name
        group: String,
    },
    /// Create a security group
    Create {
        /// Group name
        group: String,
        /// Group comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// Delete a security group
    Delete {
        /// Group name
        group: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum IpsetCommand {
    /// List all IP sets
    List,
    /// Show IP set contents
    Show {
        /// IP set name
        name: String,
    },
    /// Create an IP set
    Create {
        /// IP set name
        name: String,
        /// Comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// Delete an IP set
    Delete {
        /// IP set name
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum FirewallCommand {
    /// Cluster-level firewall rules
    #[command(subcommand)]
    Cluster(ClusterFirewallCommand),
    /// Node-level firewall rules
    #[command(subcommand)]
    Node(NodeFirewallCommand),
    /// Security groups
    Groups,
    /// Security group operations
    #[command(subcommand)]
    Group(GroupCommand),
    /// IP set operations
    #[command(subcommand)]
    Ipset(IpsetCommand),
    /// List firewall aliases
    Aliases,
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: FirewallCommand,
    global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        FirewallCommand::Cluster(sub) => match sub {
            ClusterFirewallCommand::Rules => cluster_rules(client, out).await,
            ClusterFirewallCommand::Add { rule } => {
                let params = build_rule_params(&rule);
                let param_refs: Vec<(&str, &str)> = params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let _: serde_json::Value =
                    client.post("/cluster/firewall/rules", &param_refs).await?;
                out.print_result(
                    &json!({"status": "rule added", "scope": "cluster"}),
                    "Cluster firewall rule added",
                );
                Ok(())
            }
            ClusterFirewallCommand::Delete { pos, yes } => {
                cluster_delete_rule(client, out, pos, yes).await
            }
        },
        FirewallCommand::Node(sub) => match sub {
            NodeFirewallCommand::Rules { node } => {
                let n = require_node(node.as_deref(), global_node)?;
                node_rules(client, out, n).await
            }
            NodeFirewallCommand::Add { node, rule } => {
                let n = require_node(node.as_deref(), global_node)?;
                let params = build_rule_params(&rule);
                let param_refs: Vec<(&str, &str)> = params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let path = format!("/nodes/{n}/firewall/rules");
                let _: serde_json::Value = client.post(&path, &param_refs).await?;
                out.print_result(
                    &json!({"status": "rule added", "scope": "node", "node": n}),
                    &format!("Node {n} firewall rule added"),
                );
                Ok(())
            }
            NodeFirewallCommand::Delete { node, pos, yes } => {
                let n = require_node(node.as_deref(), global_node)?;
                node_delete_rule(client, out, n, pos, yes).await
            }
        },
        FirewallCommand::Groups => groups(client, out).await,
        FirewallCommand::Group(sub) => match sub {
            GroupCommand::Show { group } => group_show(client, out, &group).await,
            GroupCommand::Create { group, comment } => {
                group_create(client, out, &group, comment.as_deref()).await
            }
            GroupCommand::Delete { group, yes } => group_delete(client, out, &group, yes).await,
        },
        FirewallCommand::Ipset(sub) => match sub {
            IpsetCommand::List => ipset_list(client, out).await,
            IpsetCommand::Show { name } => ipset_show(client, out, &name).await,
            IpsetCommand::Create { name, comment } => {
                ipset_create(client, out, &name, comment.as_deref()).await
            }
            IpsetCommand::Delete { name, yes } => ipset_delete(client, out, &name, yes).await,
        },
        FirewallCommand::Aliases => aliases(client, out).await,
    }
}

fn print_rules(data: &[serde_json::Value]) {
    let color = use_color();
    let header = format!(
        "{:>4}  {:<8}  {:<6}  {:<8}  {:<18}  {:<18}  {:<8}  COMMENT",
        "POS", "ACTION", "TYPE", "PROTO", "SOURCE", "DEST", "DPORT"
    );
    let total_w = 4 + 2 + 8 + 2 + 6 + 2 + 8 + 2 + 18 + 2 + 18 + 2 + 8 + 2 + 7; // "COMMENT" is 7 chars
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(total_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(total_w));
    }
    for rule in data {
        let pos = rule.get("pos").and_then(|v| v.as_u64()).unwrap_or(0);
        let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("-");
        let rtype = rule.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let proto = rule.get("proto").and_then(|v| v.as_str()).unwrap_or("-");
        let source = rule.get("source").and_then(|v| v.as_str()).unwrap_or("-");
        let dest = rule.get("dest").and_then(|v| v.as_str()).unwrap_or("-");
        let dport = rule.get("dport").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = rule.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        if color {
            println!(
                "{:>4}  {:<8}  {:<6}  {:<8}  {:<18}  {:<18}  {:<8}  {}",
                pos.to_string().dimmed(),
                action.bold(),
                rtype.to_string().dimmed(),
                proto.to_string().dimmed(),
                source.to_string().dimmed(),
                dest.to_string().dimmed(),
                dport.to_string().dimmed(),
                comment
            );
        } else {
            println!(
                "{:>4}  {:<8}  {:<6}  {:<8}  {:<18}  {:<18}  {:<8}  {}",
                pos, action, rtype, proto, source, dest, dport, comment
            );
        }
    }
}

fn build_rule_params(rule: &FirewallRuleArgs) -> Vec<(String, String)> {
    let mut params: Vec<(String, String)> = vec![
        ("action".to_string(), rule.action.clone()),
        ("type".to_string(), rule.r#type.clone()),
    ];
    if let Some(e) = rule.enable {
        params.push(("enable".to_string(), if e { "1" } else { "0" }.to_string()));
    }
    if let Some(ref i) = rule.iface {
        params.push(("iface".to_string(), i.clone()));
    }
    if let Some(ref s) = rule.source {
        params.push(("source".to_string(), s.clone()));
    }
    if let Some(ref d) = rule.dest {
        params.push(("dest".to_string(), d.clone()));
    }
    if let Some(ref dp) = rule.dport {
        params.push(("dport".to_string(), dp.clone()));
    }
    if let Some(ref p) = rule.proto {
        params.push(("proto".to_string(), p.clone()));
    }
    if let Some(ref m) = rule.r#macro {
        params.push(("macro".to_string(), m.clone()));
    }
    if let Some(ref c) = rule.comment {
        params.push(("comment".to_string(), c.clone()));
    }
    params
}

async fn cluster_rules(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/firewall/rules").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No cluster firewall rules found.");
        return Ok(());
    }

    print_rules(&data);
    Ok(())
}

async fn cluster_delete_rule(
    client: &ProxmoxClient,
    out: OutputConfig,
    pos: u32,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(
        &format!("delete cluster firewall rule at position {pos}"),
        yes,
    )?;

    let path = format!("/cluster/firewall/rules/{pos}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "rule deleted", "pos": pos, "scope": "cluster"}),
        &format!("Cluster firewall rule {pos} deleted"),
    );
    Ok(())
}

async fn node_rules(client: &ProxmoxClient, out: OutputConfig, node: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get(&format!("/nodes/{node}/firewall/rules")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message(&format!("No firewall rules found for node {node}."));
        return Ok(());
    }

    print_rules(&data);
    Ok(())
}

async fn node_delete_rule(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    pos: u32,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(
        &format!("delete firewall rule at position {pos} on node {node}"),
        yes,
    )?;

    let path = format!("/nodes/{node}/firewall/rules/{pos}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "rule deleted", "pos": pos, "scope": "node", "node": node}),
        &format!("Node {node} firewall rule {pos} deleted"),
    );
    Ok(())
}

async fn groups(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/firewall/groups").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No security groups found.");
        return Ok(());
    }

    let color = use_color();
    let grp_header = format!("{:<20}  COMMENT", "GROUP");
    let grp_total_w = 20 + 2 + 7; // "COMMENT" is 7 chars
    if color {
        println!("{}", grp_header.bold());
        println!("{}", "-".repeat(grp_total_w).dimmed());
    } else {
        println!("{grp_header}");
        println!("{}", "-".repeat(grp_total_w));
    }
    for g in &data {
        let name = g.get("group").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = g.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        if color {
            println!("{:<20}  {}", name.bold(), comment);
        } else {
            println!("{:<20}  {}", name, comment);
        }
    }

    Ok(())
}

async fn group_show(client: &ProxmoxClient, out: OutputConfig, group: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client
        .get(&format!("/cluster/firewall/groups/{group}"))
        .await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message(&format!("No rules in group {group}."));
        return Ok(());
    }

    println!("Security Group: {group}");
    print_rules(&data);
    Ok(())
}

async fn group_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    group: &str,
    comment: Option<&str>,
) -> Result<(), Error> {
    let mut params: Vec<(&str, &str)> = vec![("group", group)];
    if let Some(c) = comment {
        params.push(("comment", c));
    }
    let _: serde_json::Value = client.post("/cluster/firewall/groups", &params).await?;

    out.print_result(
        &json!({"status": "created", "group": group}),
        &format!("Security group {group} created"),
    );
    Ok(())
}

async fn group_delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    group: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete security group {group}"), yes)?;

    let path = format!("/cluster/firewall/groups/{group}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "group": group}),
        &format!("Security group {group} deleted"),
    );
    Ok(())
}

async fn ipset_list(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/firewall/ipset").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No IP sets found.");
        return Ok(());
    }

    let color = use_color();
    let ipset_header = format!("{:<20}  COMMENT", "IPSET");
    let ipset_total_w = 20 + 2 + 7; // "COMMENT" is 7 chars
    if color {
        println!("{}", ipset_header.bold());
        println!("{}", "-".repeat(ipset_total_w).dimmed());
    } else {
        println!("{ipset_header}");
        println!("{}", "-".repeat(ipset_total_w));
    }
    for s in &data {
        let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = s.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        if color {
            println!("{:<20}  {}", name.bold(), comment);
        } else {
            println!("{:<20}  {}", name, comment);
        }
    }

    Ok(())
}

async fn ipset_show(client: &ProxmoxClient, out: OutputConfig, name: &str) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client
        .get(&format!("/cluster/firewall/ipset/{name}"))
        .await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message(&format!("IP set {name} is empty."));
        return Ok(());
    }

    println!("IP Set: {name}");
    let color = use_color();
    let cidr_header = format!("{:<20}  COMMENT", "CIDR");
    let cidr_total_w = 20 + 2 + 7;
    if color {
        println!("{}", cidr_header.bold());
        println!("{}", "-".repeat(cidr_total_w).dimmed());
    } else {
        println!("{cidr_header}");
        println!("{}", "-".repeat(cidr_total_w));
    }
    for entry in &data {
        let cidr = entry.get("cidr").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = entry.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        if color {
            println!("{:<20}  {}", cidr.bold(), comment);
        } else {
            println!("{:<20}  {}", cidr, comment);
        }
    }

    Ok(())
}

async fn ipset_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    name: &str,
    comment: Option<&str>,
) -> Result<(), Error> {
    let mut params: Vec<(&str, &str)> = vec![("name", name)];
    if let Some(c) = comment {
        params.push(("comment", c));
    }
    let _: serde_json::Value = client.post("/cluster/firewall/ipset", &params).await?;

    out.print_result(
        &json!({"status": "created", "ipset": name}),
        &format!("IP set {name} created"),
    );
    Ok(())
}

async fn ipset_delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    name: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete IP set {name}"), yes)?;

    let path = format!("/cluster/firewall/ipset/{name}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "ipset": name}),
        &format!("IP set {name} deleted"),
    );
    Ok(())
}

async fn aliases(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/cluster/firewall/aliases").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No firewall aliases found.");
        return Ok(());
    }

    let color = use_color();
    let alias_header = format!("{:<20}  {:<20}  COMMENT", "NAME", "CIDR");
    let alias_total_w = 20 + 2 + 20 + 2 + 7; // "COMMENT" is 7 chars
    if color {
        println!("{}", alias_header.bold());
        println!("{}", "-".repeat(alias_total_w).dimmed());
    } else {
        println!("{alias_header}");
        println!("{}", "-".repeat(alias_total_w));
    }
    for a in &data {
        let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let cidr = a.get("cidr").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = a.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        if color {
            println!(
                "{:<20}  {:<20}  {}",
                name.bold(),
                cidr.to_string().dimmed(),
                comment
            );
        } else {
            println!("{:<20}  {:<20}  {}", name, cidr, comment);
        }
    }

    Ok(())
}
