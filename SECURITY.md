# Security Policy

## Overview

Celestial is a desktop Mihomo / Clash Meta client built with Tauri, React, TypeScript, and Rust. Because the application manages proxy profiles, remote subscriptions, system proxy settings, TUN mode, updater metadata, local backups, logs, and desktop integrations, security reports are especially valuable when they affect local privilege boundaries, remote content handling, updater integrity, or sensitive user data.

Please report security issues privately. Do not open a public GitHub issue with exploit details, proof-of-concept code, subscription URLs, tokens, private configuration, logs containing secrets, or user-identifying data.

## Supported Versions

Security fixes are handled for the currently maintained codebase and the latest public release only, unless maintainers explicitly announce additional supported versions.

| Version                             | Supported                                         |
| ----------------------------------- | ------------------------------------------------- |
| `main` branch                       | Best effort                                       |
| Latest public release               | Yes                                               |
| Older releases                      | No — please upgrade first                         |
| Unofficial forks or modified builds | No, unless the issue also affects this repository |

## Reporting a Vulnerability

Preferred reporting channel:

1. Use GitHub's private vulnerability reporting flow from this repository's **Security** tab, if available.
2. If private vulnerability reporting is unavailable, open a minimal public issue titled `Security contact request` without technical details, exploit steps, secrets, or screenshots. A maintainer will provide a private channel for the report.

Please include as much of the following as possible:

* Affected version, commit, or release artifact.
* Operating system and architecture.
* Whether the issue affects development builds, packaged releases, or both.
* Clear reproduction steps.
* Expected behavior versus actual behavior.
* Security impact and realistic attack scenario.
* Any proof of concept needed to reproduce the issue.
* Relevant logs or configuration snippets with secrets, access tokens, subscription URLs, and personal data removed.

## What Counts as a Security Vulnerability

Examples of in-scope reports include:

* Arbitrary code execution or command execution from remote profile data, imported YAML, merge scripts, Web UI entries, deep links, updater metadata, or other untrusted input.
* Local privilege escalation, sandbox escape, or unsafe use of elevated privileges.
* Tauri command, capability, IPC, asset protocol, filesystem, shell, process, HTTP, updater, or deep-link behavior that exposes more access than intended.
* Updater signature bypass, downgrade attacks, tampering with release artifacts, or unsafe handling of sidecar binaries.
* Exposure of secrets, subscription URLs, access tokens, proxy credentials, private rules, profile content, backups, or logs.
* Vulnerabilities in TUN mode, DNS, system proxy, controller settings, or PAC handling that allow unauthorized traffic interception, redirection, or disclosure.
* Cross-site scripting, HTML injection, or UI injection that leads to account compromise, secret disclosure, code execution, filesystem access, or privilege escalation.
* Dependency, build, packaging, or CI/CD supply-chain issues that can compromise official releases.

## Out of Scope

The following are generally out of scope unless they demonstrate a concrete, exploitable security impact in Celestial:

* Vulnerabilities that only affect an unsupported old release.
* Issues in upstream Mihomo, Clash Verge Rev, Tauri, React, Rust crates, npm packages, operating systems, or proxy providers unless Celestial introduces or uniquely exposes the vulnerability.
* Reports from automated scanners without a working proof of concept or realistic impact.
* Social engineering, phishing, spam, or attacks requiring compromise of a maintainer account.
* Denial-of-service issues that only affect the reporter's own local instance and do not cross a privilege or trust boundary.
* Attacks requiring full local administrator/root control before exploitation.
* Secrets, subscription URLs, or configuration files intentionally shared by the user with an untrusted party.
* UI/UX bugs, crashes, or configuration mistakes without a security impact.

## Response Expectations

Maintainers will make a best effort to:

* Acknowledge the report within 7 days.
* Provide an initial assessment within 14 days.
* Keep the reporter updated while a fix is being prepared.
* Coordinate disclosure timing when the issue is valid.
* Credit the reporter when requested, unless the reporter prefers to remain anonymous.

Fix timelines depend on severity, affected platforms, release complexity, upstream coordination, and maintainer availability. Critical issues may be addressed faster; low-severity or defense-in-depth issues may take longer.

## Coordinated Disclosure

Please give maintainers a reasonable opportunity to investigate and publish a fix before public disclosure. As a default guideline, avoid public disclosure for up to 90 days after a complete private report, unless both parties agree to a different timeline.

If a vulnerability is already being actively exploited or publicly discussed, mention that in the initial report so maintainers can prioritize triage.

## Safe Harbor

Good-faith security research is welcome when it follows these rules:

* Do not access, modify, delete, or exfiltrate data that does not belong to you.
* Do not disrupt services, users, maintainers, release infrastructure, or third-party systems.
* Do not publish exploit details before coordinated disclosure.
* Do not use findings for extortion, persistence, malware, credential theft, or lateral movement.
* Test only on your own systems, accounts, profiles, subscriptions, and builds unless you have explicit permission.

Reports that follow this policy will be treated as authorized security research to the extent maintainers are able to do so.

## Security Guidance for Contributors

When submitting code that affects security-sensitive areas, please be extra careful with:

* Tauri commands, permissions, capabilities, IPC, filesystem access, shell/process usage, asset protocol, updater configuration, and deep links.
* Remote profile fetching, YAML parsing, merge scripts, script execution, Web UI entries, and PAC handling.
* Logging, crash reports, backups, exported configuration, and any place where secrets may be persisted.
* Sidecar binary download, verification, packaging, release automation, and updater artifacts.
* Dependency updates, GitHub Actions, build scripts, and release scripts.

Before opening a pull request, run the relevant checks where possible:

```bash
pnpm lint
pnpm typecheck
pnpm run web:build
cargo fmt
cargo clippy --workspace --all-targets
cargo deny check
```

If a check is unavailable in your environment, mention that in the pull request.

## Security Fix Releases

Security fixes may be released without full vulnerability details until users have had time to upgrade. Release notes may describe the issue at a high level first, then include more detail after the coordinated disclosure period ends.

Users should keep Celestial updated and avoid using builds from unknown sources.
