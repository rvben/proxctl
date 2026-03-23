# proxctl

A command-line interface for [Proxmox VE](https://www.proxmox.com/en/proxmox-virtual-environment/overview) — manage VMs, containers, nodes, storage, and more from your terminal.

## Install

```bash
# From crates.io
cargo install proxctl

# From PyPI
pip install proxctl
```

## Quick Start

```bash
# Interactive setup (creates API token automatically)
proxctl config init

# Check connectivity
proxctl health

# List VMs
proxctl vm list

# Start a VM
proxctl vm start 100

# Raw API access
proxctl api get /nodes
```

## Features

- **120+ commands** covering VMs, containers, nodes, storage, backups, cluster, firewall, access control, pools, and Ceph
- **Auto-detection** — resolves which node a VM lives on automatically
- **Agent-friendly** — `--json` output, `schema` command for introspection, structured exit codes
- **Async task handling** — waits for operations to complete with progress spinner
- **Safe** — destructive operations require `--yes` confirmation
- **Raw API escape hatch** — `proxctl api get/post/put/delete` for any endpoint

## Configuration

Config file: `~/.config/proxctl/config.toml`

```toml
[default]
host = "https://192.168.1.1:8006"
token = "root@pam!proxctl=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
insecure = true

[production]
host = "https://pve.example.com:8006"
token = "admin@pam!proxctl=yyyyyyyy-..."
```

Environment variables: `PROXMOX_HOST`, `PROXMOX_TOKEN`, `PROXMOX_PROFILE`

## License

MIT
