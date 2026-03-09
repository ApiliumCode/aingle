# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in AIngle, please report it responsibly.

**Email**: security@apilium.com

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

## Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial assessment**: Within 7 days
- **Patch release**: Within 30 days (critical vulnerabilities may be expedited)

## Disclosure Policy

- Do not disclose the vulnerability publicly until a patch has been released
- We will credit reporters in the security advisory (unless anonymity is requested)
- We do not pursue legal action against good-faith security researchers

## Scope

This policy covers all code in the AIngle repository, including:
- All crates under `crates/`
- Build scripts and CI/CD configurations
- Documentation that may expose sensitive implementation details

## Cryptographic Components

AIngle uses the following cryptographic libraries:
- `blake3` — Content hashing
- `ed25519-dalek` — Digital signatures
- `rustls` — TLS connections
- Custom zero-knowledge proof implementations (Pedersen, Schnorr, Bulletproofs)

Issues in these components are considered high priority.

## Contact

Apilium Technologies OÜ
security@apilium.com
https://apilium.com/en/security
