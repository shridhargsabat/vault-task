# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

Only the latest minor release line receives security fixes. If you are on an
older version, please upgrade and confirm the issue still reproduces before
reporting.

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.** Use one of these
private channels:

1. **Preferred:** [Open a private security
   advisory](https://github.com/shridhargsabat/vault-task/security/advisories/new)
2. **Fallback:** Email shridhargsabat@gmail.com with subject
   `[vault-task security] <brief description>`

Please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce (OS, install method, exact clicks/commands — redact
  any secrets, and never send your real vault password)
- The VaultTask version and install method (`.msi`, `.exe`, `.deb`,
  `.rpm`, `.AppImage`, built from source)
- Any suggested mitigation, if you have one

### What to expect

- **Acknowledgement** within 3 business days
- **Status updates** at least every 7 days until resolved
- **Coordinated disclosure:** we will agree on a fix + release timeline with
  you before any public disclosure, and credit you in the release notes unless
  you prefer to stay anonymous

## Scope

In scope:

- Vault cryptography (`src-tauri/src/crypto.rs`: Argon2id KDF,
  AES-256-GCM) and key handling (`state.rs`, zeroization on lock)
- The auto-lock engine (`watchdog.rs`), session-lock listeners, and the
  frontend memory purge on `vault://locked`
- The Tauri IPC surface (`commands.rs`) and the encrypted SQLite store
- Release workflows and installer packaging in this repository

Out of scope:

- Vulnerabilities in WebKitGTK / WebView2 themselves (report those
  upstream), unless VaultTask configures them unsafely
- Vulnerabilities in dependencies without a demonstrating exploit path
  through VaultTask itself — though we still appreciate the heads-up so
  we can upgrade
- Lost vault passwords: by design there is no recovery and no backdoor;
  a forgotten password means the data is unrecoverable

## Notes for vault-security reports

The most useful reports demonstrate plaintext exposure: decrypted task/note
bytes surviving a lock (memory, DOM, logs, or disk outside the encrypted
SQLite file), key material in crash dumps, or the auto-lock failing to
trigger. When testing, use a throwaway vault password.
