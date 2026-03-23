use clap::Subcommand;
use serde_json::json;

use crate::api::Error;
use crate::api::client::ProxmoxClient;
use crate::output::OutputConfig;

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
pub enum UserCommand {
    /// Show user details
    Show {
        /// User ID (e.g. root@pam)
        userid: String,
    },
    /// Create a user
    Create {
        /// User ID (e.g. user@realm)
        userid: String,
        /// User comment
        #[arg(long)]
        comment: Option<String>,
        /// Email address
        #[arg(long)]
        email: Option<String>,
        /// Enable the user
        #[arg(long)]
        enable: Option<bool>,
        /// First name
        #[arg(long)]
        firstname: Option<String>,
        /// Last name
        #[arg(long)]
        lastname: Option<String>,
    },
    /// Delete a user
    Delete {
        /// User ID
        userid: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum TokenCommand {
    /// List tokens for a user
    List {
        /// User ID (e.g. root@pam)
        userid: String,
    },
    /// Create a token for a user
    Create {
        /// User ID (e.g. root@pam)
        userid: String,
        /// Token ID
        tokenid: String,
        /// Token comment
        #[arg(long)]
        comment: Option<String>,
        /// Token expiration (epoch)
        #[arg(long)]
        expire: Option<u64>,
        /// Privilege separation
        #[arg(long)]
        privsep: Option<bool>,
    },
    /// Delete a token
    Delete {
        /// User ID
        userid: String,
        /// Token ID
        tokenid: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum AccessCommand {
    /// List all users
    Users,
    /// User operations
    #[command(subcommand)]
    User(UserCommand),
    /// List all roles
    Roles,
    /// Show access control list
    Acl,
    /// API token operations
    #[command(subcommand)]
    Token(TokenCommand),
}

pub async fn run(
    client: &ProxmoxClient,
    out: OutputConfig,
    cmd: AccessCommand,
    _global_node: Option<&str>,
) -> Result<(), Error> {
    match cmd {
        AccessCommand::Users => users(client, out).await,
        AccessCommand::User(sub) => match sub {
            UserCommand::Show { userid } => user_show(client, out, &userid).await,
            UserCommand::Create {
                userid,
                comment,
                email,
                enable,
                firstname,
                lastname,
            } => {
                user_create(
                    client,
                    out,
                    &userid,
                    comment.as_deref(),
                    email.as_deref(),
                    enable,
                    firstname.as_deref(),
                    lastname.as_deref(),
                )
                .await
            }
            UserCommand::Delete { userid, yes } => user_delete(client, out, &userid, yes).await,
        },
        AccessCommand::Roles => roles(client, out).await,
        AccessCommand::Acl => acl(client, out).await,
        AccessCommand::Token(sub) => match sub {
            TokenCommand::List { userid } => token_list(client, out, &userid).await,
            TokenCommand::Create {
                userid,
                tokenid,
                comment,
                expire,
                privsep,
            } => {
                token_create(
                    client,
                    out,
                    &userid,
                    &tokenid,
                    comment.as_deref(),
                    expire,
                    privsep,
                )
                .await
            }
            TokenCommand::Delete {
                userid,
                tokenid,
                yes,
            } => token_delete(client, out, &userid, &tokenid, yes).await,
        },
    }
}

async fn users(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/access/users").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No users found.");
        return Ok(());
    }

    println!(
        "{:<30}  {:<8}  {:<30}  COMMENT",
        "USERID", "ENABLE", "EMAIL"
    );
    for user in &data {
        let userid = user.get("userid").and_then(|v| v.as_str()).unwrap_or("-");
        let enable = user
            .get("enable")
            .and_then(|v| v.as_u64())
            .map(|v| if v == 1 { "yes" } else { "no" })
            .unwrap_or("yes");
        let email = user.get("email").and_then(|v| v.as_str()).unwrap_or("-");
        let comment = user.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<30}  {:<8}  {:<30}  {}", userid, enable, email, comment);
    }

    Ok(())
}

async fn user_show(client: &ProxmoxClient, out: OutputConfig, userid: &str) -> Result<(), Error> {
    let data: serde_json::Value = client.get(&format!("/access/users/{userid}")).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    println!("User: {userid}");
    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            println!("  {key}: {val_str}");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn user_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    userid: &str,
    comment: Option<&str>,
    email: Option<&str>,
    enable: Option<bool>,
    firstname: Option<&str>,
    lastname: Option<&str>,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = vec![("userid".to_string(), userid.to_string())];
    if let Some(c) = comment {
        params.push(("comment".to_string(), c.to_string()));
    }
    if let Some(e) = email {
        params.push(("email".to_string(), e.to_string()));
    }
    if let Some(en) = enable {
        params.push(("enable".to_string(), if en { "1" } else { "0" }.to_string()));
    }
    if let Some(f) = firstname {
        params.push(("firstname".to_string(), f.to_string()));
    }
    if let Some(l) = lastname {
        params.push(("lastname".to_string(), l.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let _: serde_json::Value = client.post("/access/users", &param_refs).await?;

    out.print_result(
        &json!({"status": "created", "userid": userid}),
        &format!("User {userid} created"),
    );
    Ok(())
}

async fn user_delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    userid: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete user {userid}"), yes)?;

    let path = format!("/access/users/{userid}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "userid": userid}),
        &format!("User {userid} deleted"),
    );
    Ok(())
}

async fn roles(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/access/roles").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No roles found.");
        return Ok(());
    }

    println!("{:<20}  PRIVS", "ROLEID");
    for role in &data {
        let roleid = role.get("roleid").and_then(|v| v.as_str()).unwrap_or("-");
        let privs = role.get("privs").and_then(|v| v.as_str()).unwrap_or("-");
        println!("{:<20}  {}", roleid, privs);
    }

    Ok(())
}

async fn acl(client: &ProxmoxClient, out: OutputConfig) -> Result<(), Error> {
    let data: Vec<serde_json::Value> = client.get("/access/acl").await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message("No ACL entries found.");
        return Ok(());
    }

    println!(
        "{:<30}  {:<25}  {:<20}  PROPAGATE",
        "PATH", "UGID", "ROLEID"
    );
    for entry in &data {
        let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("-");
        let ugid = entry.get("ugid").and_then(|v| v.as_str()).unwrap_or("-");
        let roleid = entry.get("roleid").and_then(|v| v.as_str()).unwrap_or("-");
        let propagate = entry
            .get("propagate")
            .and_then(|v| v.as_u64())
            .map(|v| if v == 1 { "yes" } else { "no" })
            .unwrap_or("-");
        println!("{:<30}  {:<25}  {:<20}  {}", path, ugid, roleid, propagate);
    }

    Ok(())
}

async fn token_list(client: &ProxmoxClient, out: OutputConfig, userid: &str) -> Result<(), Error> {
    let path = format!("/access/users/{userid}/token");
    let data: Vec<serde_json::Value> = client.get(&path).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
        return Ok(());
    }

    if data.is_empty() {
        out.print_message(&format!("No tokens found for user {userid}."));
        return Ok(());
    }

    println!("{:<20}  {:<8}  COMMENT", "TOKENID", "PRIVSEP");
    for token in &data {
        let tokenid = token.get("tokenid").and_then(|v| v.as_str()).unwrap_or("-");
        let privsep = token
            .get("privsep")
            .and_then(|v| v.as_u64())
            .map(|v| if v == 1 { "yes" } else { "no" })
            .unwrap_or("-");
        let comment = token.get("comment").and_then(|v| v.as_str()).unwrap_or("");
        println!("{:<20}  {:<8}  {}", tokenid, privsep, comment);
    }

    Ok(())
}

async fn token_create(
    client: &ProxmoxClient,
    out: OutputConfig,
    userid: &str,
    tokenid: &str,
    comment: Option<&str>,
    expire: Option<u64>,
    privsep: Option<bool>,
) -> Result<(), Error> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(c) = comment {
        params.push(("comment".to_string(), c.to_string()));
    }
    if let Some(e) = expire {
        params.push(("expire".to_string(), e.to_string()));
    }
    if let Some(p) = privsep {
        params.push(("privsep".to_string(), if p { "1" } else { "0" }.to_string()));
    }

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let path = format!("/access/users/{userid}/token/{tokenid}");
    let data: serde_json::Value = client.post(&path, &param_refs).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&data).expect("serialize"));
    } else {
        let value = data.get("value").and_then(|v| v.as_str()).unwrap_or("-");
        println!("Token created: {userid}!{tokenid}");
        println!("  Secret: {value}");
        println!("  Full token: {userid}!{tokenid}={value}");
        out.print_message("Save this token secret now -- it cannot be retrieved later.");
    }

    Ok(())
}

async fn token_delete(
    client: &ProxmoxClient,
    out: OutputConfig,
    userid: &str,
    tokenid: &str,
    yes: bool,
) -> Result<(), Error> {
    confirm_action(&format!("delete token {tokenid} for user {userid}"), yes)?;

    let path = format!("/access/users/{userid}/token/{tokenid}");
    let _: serde_json::Value = client.delete(&path).await?;

    out.print_result(
        &json!({"status": "deleted", "userid": userid, "tokenid": tokenid}),
        &format!("Token {tokenid} for user {userid} deleted"),
    );
    Ok(())
}
