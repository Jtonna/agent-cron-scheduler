# ACS Documentation

Documentation for the Agent Cron Scheduler (ACS) -- a cross-platform cron scheduling daemon written in Rust.

## Documents

| Document | Covers |
|----------|--------|
| [Architecture](architecture.md) | System overview, module structure, data flow diagrams, concurrency model, key design decisions. |
| [Configuration](configuration.md) | Config file format, field reference, config resolution order, data directory locations, environment variables. |
| [CLI Reference](cli-reference.md) | All `agentcronsystem` subcommands: flags, options, exit codes, usage examples. |
| [API Reference](api-reference.md) | REST API endpoints: routes, request/response formats, status codes, SSE events, data models. |
| [Workflow Management](workflow-management.md) | Workflow definitions, step kinds (Shell/Script/Http/Match/SetVar/Agent), template substitution, run lifecycle, failure policies, cron expressions. |
| [Service Registration](service-registration.md) | Platform-specific service setup: Windows Registry Run key, macOS launchd, Linux systemd. |
| [Storage](storage.md) | On-disk persistence: WorkflowStore, WorkflowRunStore, FileLogSink, EventEmittingLogSink, atomic writes, corruption handling, migration system, daemon log management. |
| [Troubleshooting](troubleshooting.md) | Common problems and solutions: startup issues, workflow execution, logs, data corruption, CLI errors. |
| [Known Issues](KNOWN_ISSUES.md) | Remaining documentation issues identified during Round 3 audits; minor items that do not affect core accuracy. |
