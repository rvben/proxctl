# Changelog

All notable changes to this project will be documented in this file.

## [0.1.1] - 2026-03-23

### Added
- Full VM command module: list, status, start, stop, shutdown, reboot, reset, suspend, resume, config, set, create, destroy, clone, migrate, template, resize, console, snapshots, guest agent, firewall, cloud-init
- Full container command module mirroring VM structure for LXC containers
- Node management: list, status, shutdown, reboot, start-all, stop-all, services, network, disks, syslog, apt, certificates
- Task management: list, status, log, stop, wait
- Storage management: list, status, content, download, create, update, delete
- Backup management: list, create, restore, schedule CRUD
- Cluster operations: status, resources, nextid, log, options, HA resources/status
- Firewall management: cluster/node rules, security groups, IP sets, aliases
- Access control: users, roles, ACL, API tokens
- Resource pool management: list, show, create, update, delete
- Ceph management: status, OSD, pools, monitors
- Raw API passthrough: get, post, put, delete
- Interactive config init with automatic API token creation (privsep=0)
- Config show command with token masking
- Auto-generated agent introspection schema (145 commands) with types, defaults, enums, and behavioral metadata
- CI/CD: GitHub Actions for lint/test and cross-platform release (Linux x86/arm, macOS x86/arm, Windows)
- Published to crates.io, PyPI, and GitHub Releases
- Hidden aliases: `qm` for `vm`, `ct` for `container`
- VMID-to-node auto-resolution via cluster resources cache
- Idempotent lifecycle commands (start on running = success)
- Destructive operations require --yes confirmation

### Fixed
- Token API path corrected from `/tokens/` to `/token/`
- API tokens created with privsep=0 to inherit user permissions

## [0.1.0] - 2026-03-23

### Added
- Initial release with core infrastructure
