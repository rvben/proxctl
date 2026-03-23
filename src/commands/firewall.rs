use clap::Subcommand;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

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
            ClusterFirewallCommand::Add {
                action,
                r#type,
                enable,
                source,
                dest,
                dport,
                proto,
                comment,
            } => {
                cluster_add_rule(
                    client,
                    out,
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
            ClusterFirewallCommand::Delete { pos, yes } => {
                cluster_delete_rule(client, out, pos, yes).await
            }
        },
        FirewallCommand::Node(sub) => match sub {
            NodeFirewallCommand::Rules { node } => {
                let n = require_node(node.as_deref(), global_node)?;
                node_rules(client, out, n).await
            }
            NodeFirewallCommand::Add {
                node,
                action,
                r#type,
                enable,
                source,
                dest,
                dport,
                proto,
                comment,
            } => {
                let n = require_node(node.as_deref(), global_node)?;
                node_add_rule(
                    client,
                    out,
                    n,
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
    println!(
        "{:>4}  {:<8}  {:<6}  {:<8}  {:<18}  {:<18}  {:<8}  COMMENT",
        "POS", "ACTION", "TYPE", "PROTO", "SOURCE", "DEST", "DPORT"
    );
    for rule in data {
        let pos = rule.get("pos").and_then(|v| v.as_u64()).unwrap_or(0);
        let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("-");
        let rtype = rule.get("type").and_then(|v| v.as_str()).unwrap_or("-");
        let proto = rule.get("proto").and_then(|v| v.as_str()).unwrap_or("-");
        let source = rule.get("source").and_then(|v| v.as_str()).unwrap_or("-");
        let dest = rule.get("dest").and_then(|v| v.as_str()).unwrap_or("-");
        let dport = rule.get("dport").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = rule.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!(
            "{:>4}  {:<8}  {:<6}  {:<8}  {:<18}  {:<18}  {:<8}  {}",
            pos, action, rtype, proto, source, dest, dport, comment
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn build_rule_params(
    action: &str,
    rule_type: &str,
    enable: Option<bool>,
    source: Option<&str>,
    dest: Option<&str>,
    dport: Option<&str>,
    proto: Option<&str>,
    comment: Option<&str>,
) -> Vec<(String, String)> {
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

#[allow(clippy::too_many_arguments)]
async fn cluster_add_rule(
    client: &ProxmoxClient,
    out: OutputConfig,
    action: &str,
    rule_type: &str,
    enable: Option<bool>,
    source: Option<&str>,
    dest: Option<&str>,
    dport: Option<&str>,
    proto: Option<&str>,
    comment: Option<&str>,
) -> Result<(), Error> {
    let params = build_rule_params(
        action, rule_type, enable, source, dest, dport, proto, comment,
    );
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let _: serde_json::Value = client.post("/cluster/firewall/rules", &param_refs).await?;

    out.print_result(
        &json!({"status": "rule added", "scope": "cluster"}),
        "Cluster firewall rule added",
    );
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

#[allow(clippy::too_many_arguments)]
async fn node_add_rule(
    client: &ProxmoxClient,
    out: OutputConfig,
    node: &str,
    action: &str,
    rule_type: &str,
    enable: Option<bool>,
    source: Option<&str>,
    dest: Option<&str>,
    dport: Option<&str>,
    proto: Option<&str>,
    comment: Option<&str>,
) -> Result<(), Error> {
    let params = build_rule_params(
        action, rule_type, enable, source, dest, dport, proto, comment,
    );
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/nodes/{node}/firewall/rules");
    let _: serde_json::Value = client.post(&path, &param_refs).await?;

    out.print_result(
        &json!({"status": "rule added", "scope": "node", "node": node}),
        &format!("Node {node} firewall rule added"),
    );
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

    println!("{:<20}  COMMENT", "GROUP");
    for g in &data {
        let name = g.get("group").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = g.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {}", name, comment);
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

    println!("{:<20}  COMMENT", "IPSET");
    for s in &data {
        let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = s.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {}", name, comment);
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
    println!("{:<20}  COMMENT", "CIDR");
    for entry in &data {
        let cidr = entry.get("cidr").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = entry.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {}", cidr, comment);
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

    println!("{:<20}  {:<20}  COMMENT", "NAME", "CIDR");
    for a in &data {
        let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let cidr = a.get("cidr").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = a.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {:<20}  {}", name, cidr, comment);
    }

    Ok(())
}
