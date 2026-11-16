# Security policy

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:

https://github.com/FieldmouseWorks/callcraft/security/advisories/new

Do not open a public issue for a security report.

## Supported versions

Only the latest release and `main` are supported. Security fixes land on `main`
and ship in the next release.

## Scope

- The reference implementation: format handling, loopback server, CLI, and
  Tauri desktop shell.
- The scenario format and JSON Schema.
- Out of scope: the third-party provider services themselves (their outages,
  data handling, and terms), hosted deployments operated by others, and any
  live-call or CJI integration (none exists by design).

## Dependency policy

[`docs/DEPENDENCY-POLICY.md`](docs/DEPENDENCY-POLICY.md) defines update SLAs and
CI gates. Advisories are treated as defects.
