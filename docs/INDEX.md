# ACS Documentation

Documentation for the Agent Cron Scheduler (ACS) -- a cross-platform cron scheduling daemon written in Rust.

## Documents

| Document | Covers |
|----------|--------|
| [Architecture](architecture.md) | System overview, module structure, data flow diagrams, concurrency model, key design decisions. |
| [Configuration](configuration.md) | Config file format, field reference, config resolution order, data directory locations, environment variables. |
| [CLI Reference](cli-reference.md) | All `acs` subcommands: flags, options, exit codes, usage examples. |
| [API Reference](api-reference.md) | REST API endpoints: routes, request/response formats, status codes, SSE events, data models. |
| [Job Management](job-management.md) | Job model, execution types, cron expressions, timezone support, job lifecycle, validation rules. |
| [Service Registration](service-registration.md) | Platform-specific service setup: Windows Task Scheduler, macOS launchd, Linux systemd. |
| [Storage](storage.md) | On-disk persistence: JsonJobStore, FsLogStore, file formats, log rotation, daemon log management, storage traits. |
| [Troubleshooting](troubleshooting.md) | Common problems and solutions: startup issues, job execution, logs, data corruption, CLI errors. |
| [Beads → GitHub Sync](beads-sync.md) | How local beads issues sync to GitHub Issues and the Projects kanban board on merge to main. |
