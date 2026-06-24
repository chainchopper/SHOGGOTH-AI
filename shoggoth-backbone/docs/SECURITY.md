# Shoggoth Backbone — Security Audit
#
# Documents the security posture of the Shoggoth Mesh Machine.
# Updated: 2026-06-24

## Dependency Auditing

```bash
# Install audit tools
cargo install cargo-audit cargo-deny

# Run security audits
cargo audit --manifest-path shoggoth-backbone/Cargo.toml
cargo deny check advisories --manifest-path shoggoth-backbone/Cargo.toml
cargo deny check licenses --manifest-path shoggoth-backbone/Cargo.toml
```

## Attack Surface

| Component | Exposure | Mitigation |
|-----------|----------|------------|
| REST API (port 9100) | Network | API key auth + OIDC/SSO + rate limiting |
| WebSocket (port 9101) | Network | Authenticated upgrade via session cookie |
| QUIC control plane | Network | mTLS with certificate pinning |
| UDP heartbeat (port 8888) | LAN only | MAC-filtered at switch; encrypted payload via QAT AES-256-GCM |
| DMA-BUF fd passing | Local kernel | Requires CAP_SYS_ADMIN; fd caps: O_CLOEXEC |
| `/dev/dri/renderD*` | Local | SELinux/AppArmor MAC policy |
| Shared memory (`/dev/shm/shoggoth/`) | Local | Unix permissions 0600; shoggoth group only |
| Vsock (AF_HYPERV) | Hypervisor | VM boundary; WSL2 isolates guest from host |

## Secure Development Practices

- All `unsafe` blocks require `// SAFETY:` comments citing kernel/driver guarantees.
- `#[deny(unsafe_code)]` on all crates except `shoggoth-core`.
- Dependencies audited weekly via `cargo audit` in CI.
- No `unwrap()` or `expect()` in production code — use proper error propagation.
- API keys are HMAC-SHA256 hashed; raw keys are never logged.
- TLS certificates generated via rcgen with 2048-bit RSA keys, rotated annually.
- QAT AES-256-GCM encryption for inter-node traffic with per-message nonces.

## Vulnerability Reporting

Email: security@shoggoth.dev  
PGP Key: [Link to public key]  
Response SLA: 72 hours acknowledgment, 7 days fix for critical vulnerabilities.

See also: [GitHub Security Advisories](https://github.com/chainchopper/shoggoth-backbone/security/advisories)
