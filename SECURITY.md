# Security Policy

## Supported Versions

AgentDoctor is pre-1.0. Security fixes are expected to target the latest
released version.

## Reporting a Vulnerability

Please do not open a public issue for security vulnerabilities.

Use GitHub private vulnerability reporting:

https://github.com/youssefsz/agentdoctor/security/advisories/new

Include:

- affected version or commit
- operating system
- reproduction steps
- expected and actual behavior
- whether sensitive repository data can be exposed

## Security Principles

AgentDoctor should not:

- send telemetry
- call AI APIs
- access the network during scans
- print secret values from `.env` files
- modify repository files during `scan`
