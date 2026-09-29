# Security Policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability or exposed credential.

Use GitHub's private vulnerability reporting feature in the **Security** tab of this repository. Include the affected component, reproduction steps, impact, and any proposed remediation.

Never include production credentials, private keys, seed phrases, customer data, or live webhook secrets in a report.

## Key-handling boundary

FlowPay source code must not contain private keys. Runtime secrets belong in an external secret manager or a local environment file excluded by `.gitignore`. Examples must use inert placeholder values only.
