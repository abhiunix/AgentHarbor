# Security Policy

AgentHarbor handles OAuth tokens for multiple AI providers and stores secrets in the OS keychain, so we take vulnerability reports seriously.

## Supported versions

Only the [latest release](https://github.com/abhiunix/AgentHarbor/releases/latest) receives security fixes. The app auto-updates by default.

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately via [GitHub security advisories](https://github.com/abhiunix/AgentHarbor/security/advisories/new). Include the app version, OS, and reproduction steps. You should get an initial response within 7 days.

Especially relevant areas: token storage (`provider-tokens.json`, keychain entries), the secrets manager, outbound request destinations (the app should only ever call official provider APIs), and the updater signature chain.
