# Security Policy

## Supported versions

terminal-delight is pre-1.0; security fixes land on `main` and in the latest
`0.x` tag. There is no back-port guarantee for older tags yet.

| Version | Supported |
|---------|-----------|
| `main` / latest `0.x` | ✅ |
| older tags | ❌ |

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report privately to **tools@intellimass.ai** (or use GitHub's
[private vulnerability reporting](https://github.com/parker-brown-family/terminal-delight/security/advisories/new)
if enabled). Include:

- a description of the issue and its impact,
- steps to reproduce (a minimal terminal session or theme file if relevant),
- the commit/tag you observed it on.

You'll get an acknowledgement within a few days. Once a fix is ready we'll
coordinate a disclosure timeline with you and credit you in the release notes
unless you'd prefer to stay anonymous.

## Scope notes

terminal-delight runs your real shell on a real PTY and renders untrusted
program output (ANSI/OSC byte streams). Parser-level issues — escape sequences
that crash, hang, or read/write outside the grid — are in scope. So are theme
files that can read or execute beyond the documented schema. The vendored
Zed/gpui graph is upstream's; please report renderer/framework issues to
[zed-industries/zed](https://github.com/zed-industries/zed) directly, but feel
free to flag them here too if they affect us specifically.
