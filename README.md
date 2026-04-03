# proxctl

[![crates.io](https://img.shields.io/crates/v/proxctl.svg)](https://crates.io/crates/proxctl)
[![CI](https://github.com/rvben/proxctl/actions/workflows/ci.yml/badge.svg)](https://github.com/rvben/proxctl/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![codecov](https://codecov.io/gh/rvben/proxctl/graph/badge.svg)](https://codecov.io/gh/rvben/proxctl)

A command-line interface for [Proxmox VE](https://www.proxmox.com/en/proxmox-virtual-environment/overview) -- manage VMs, containers, nodes, storage, and more from your terminal. Includes declarative infrastructure management with `apply` and `export`.

## Install

```bash
# From crates.io
cargo install proxctl

# From PyPI (pre-built binaries, no Rust toolchain needed)
pip install proxctl

# From GitHub Releases (Linux, macOS, Windows)
curl -fsSL https://github.com/rvben/proxctl/releases/latest/download/proxctl-$(uname -m)-unknown-linux-gnu.tar.gz | tar xz
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

# Show VM configuration
proxctl vm config 100

# List snapshots
proxctl vm snapshot list 100

# Raw API access
proxctl api get /nodes
```

## Declarative Infrastructure (IaC)

Manage Proxmox resources declaratively with YAML manifests, similar to `kubectl apply`.

### Export existing resources

```bash
# Export a single VM
proxctl export vm 101 > haos.yaml

# Export all containers
proxctl export container --all > containers.yaml

# Export cluster firewall rules
proxctl export firewall cluster > firewall.yaml
```

### Apply desired state

```yaml
# infra/web.yaml
kind: vm
name: web-01
vmid: 100
config:
  memory: 4096
  cores: 2
  onboot: true
---
kind: firewall-rule
scope: cluster
config:
  action: ACCEPT
  type: in
  proto: tcp
  dport: "443"
  comment: "Allow HTTPS"
```

```bash
# Preview changes
proxctl apply -f infra/ --dry-run

# Apply changes
proxctl apply -f infra/

# Round-trip: export, then verify nothing drifted
proxctl export vm --all > current.yaml
proxctl apply -f current.yaml --dry-run  # should show "noop" for all
```

### Key behaviors

- **Idempotent** -- running apply twice produces "up to date" on the second run
- **Patch semantics** -- only specified config keys are changed, others left untouched
- **Name or VMID** -- resources can be identified by name (auto-resolves VMID) or pinned by ID
- **Multi-document** -- multiple resources in one file with `---` separators, or a directory of files
- **Optional power state** -- add `state: running` or `state: stopped` to manage power, or omit to leave it alone
- **Safe** -- shows a diff before applying, destructive changes prompt for confirmation

## Features

- **145+ commands** covering VMs, containers, nodes, storage, backups, cluster, firewall, access control, pools, and Ceph
- **Declarative IaC** -- `apply` and `export` for infrastructure-as-code workflows
- **Auto-detection** -- resolves which node a VM lives on automatically
- **Agent-friendly** -- `--json` output, `schema` command for introspection, structured exit codes
- **Async task handling** -- waits for operations to complete with progress spinner
- **Safe** -- destructive operations require `--yes` confirmation
- **Raw API escape hatch** -- `proxctl api get/post/put/delete` for any endpoint
- **Hidden aliases** -- `qm` for `vm`, `ct` for `container`
- **Idempotent** -- starting an already-running VM succeeds without error

## Configuration

### Config file

`~/.config/proxctl/config.toml`

```toml
[default]
host = "https://192.168.1.1:8006"
token = "root@pam!proxctl=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
insecure = true

[production]
host = "https://pve.example.com:8006"
token = "admin@pam!proxctl=yyyyyyyy-..."
```

### Environment variables

| Variable | Description |
|---|---|
| `PROXMOX_HOST` | Proxmox host (e.g. `pve.example.com:8006`) |
| `PROXMOX_TOKEN` | API token (`user@realm!tokenid=secret`) |
| `PROXMOX_PROFILE` | Config profile name (default: `default`) |
| `PROXMOX_NODE` | Default node name |

### Precedence

CLI flags > environment variables > config file

## Usage Examples

### Human output (TTY)

```
$ proxctl vm list
  VMID  NAME                  STATUS      NODE        CPUS      MEMORY
   100  k8s-control-1         running     pve1           4    8.00 GiB
   101  k8s-worker-1          running     pve1           8   16.00 GiB
   200  dev-sandbox            stopped     pve2           2    4.00 GiB
```

### JSON output (piped or `--json`)

```bash
$ proxctl vm list --json | jq '.[].name'
"k8s-control-1"
"k8s-worker-1"
"dev-sandbox"
```

JSON output is automatic when stdout is not a TTY, so piping to `jq`, `grep`, or scripts works without flags.

## Agent Integration

The `schema` command outputs a JSON description of all 145+ commands with their arguments, types, defaults, and behavioral metadata:

```bash
proxctl schema | jq '.commands | length'
145
```

This enables AI agents and automation tools to discover available operations, required parameters, and which commands are mutating or destructive -- without parsing help text.

## Comparison

| Feature | proxctl | pvesh (built-in) | proxmoxer (Python) |
|---|---|---|---|
| Declarative IaC (apply/export) | Yes | No | No |
| Typed CLI with completions | Yes | No | N/A |
| Cross-platform binaries | Yes | No (PVE only) | pip install |
| VMID auto-resolution | Yes | No | Manual |
| JSON + human output | Auto-detect | JSON only | N/A |
| Agent schema introspection | Yes | No | No |
| Idempotent lifecycle ops | Yes | No | Manual |

## License

MIT
