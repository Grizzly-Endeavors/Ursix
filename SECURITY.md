# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Ursix, please report it responsibly:

1. **Do not** open a public GitHub issue for security vulnerabilities
2. Email security concerns to: [security@grizzly-endeavors.com](mailto:security@grizzly-endeavors.com)
3. Include a detailed description of the vulnerability and steps to reproduce
4. Allow up to 72 hours for an initial response

We will work with you to understand the issue and develop a fix before any public disclosure.

## Scope

Security issues we care about:

- Command injection via user input
- Arbitrary file read/write outside intended scope
- Credential exposure in logs or output
- Vulnerabilities in dependencies

Issues that are **not** in scope:

- Denial of service (Ursix is a CLI tool, not a service)
- Social engineering
- Issues requiring physical access

## Credential Handling

Ursix handles API keys for LLM providers. Our security measures:

- **API keys are never logged** at any log level
- **API keys are never included in JSON output** (`usx config` shows `[set]` or `[not set]`)
- **API keys are not stored in config files** by design; they must come from environment variables or CLI flags
- **HTTPS is enforced** for remote endpoints (HTTP triggers a warning for non-localhost URLs)

### Best Practices for Users

- Use environment variables (`URSIX_API_KEY`) rather than CLI flags to avoid shell history exposure
- Never commit `.ursix/config.toml` files containing credentials (they shouldn't have any, but check)
- Use separate API keys for CI/CD with minimal permissions

## Dependency Security

We use `cargo deny` to check for:

- Known vulnerabilities in dependencies
- Problematic licenses
- Unmaintained crates

The check runs in CI and as part of the pre-push hook.
