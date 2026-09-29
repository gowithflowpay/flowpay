# Backend configuration

FlowPay backend configuration is environment-driven. The canonical development template is the repository-root `.env.example`; local chain deployment additionally writes non-secret test addresses to `runtime/local.env`.

Do not place private keys, API keys, seed phrases, or production secrets in this directory.
