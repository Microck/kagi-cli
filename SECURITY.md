# Security Policy

## Supported Versions

Because this project has not been publicly released yet, only the latest commit on the default branch should be treated as supported.

## Reporting a Vulnerability

Do not open a public issue for vulnerabilities involving:

- API tokens
- Session tokens
- authenticated request flows
- accidental secret exposure

Preferred reporting path after the repository is published:

1. Use GitHub Security Advisories or another private disclosure channel if one is enabled
2. Include impact, reproduction steps, and any required environment details
3. Redact tokens, cookies, and personal data from all reports

If no private disclosure path exists yet, contact the maintainer privately before publishing details. Until that contact is defined, treat the lack of a private channel as a publishing blocker for sensitive issues.

## Secret Handling

- Never commit `.env`, `.kagi.toml`, or token-bearing logs
- Use [`.env.example`](.env.example) for safe examples
- Prefer short-lived test credentials when manual verification is unavoidable
