# Security Policy

## Supported Versions

Only the latest minor release line receives security fixes. Older minor lines
are not maintained.

| Version | Supported          |
| ------- | ------------------ |
| 6.2.x   | :white_check_mark: |
| < 6.2   | :x:                |

## Reporting a Vulnerability

Please report security vulnerabilities **privately**. Do not open a public
GitHub issue, pull request, or discussion that describes the vulnerability,
reproduction steps, or the affected code path.

Use GitHub's Private Vulnerability Reporting on this repository:

- <https://github.com/langkebo/synapse-rust/security/advisories/new>

If that form is not available to you, open a public issue that asks a
maintainer to establish a private channel — and share **no** vulnerability
details in it.

### What to include

A report is easiest to act on when it contains:

- the affected version or commit hash;
- the component (route, service, storage, federation, E2EE, …);
- a description of the impact and the conditions needed to trigger it;
- minimal reproduction steps, and any proof-of-concept input;
- whether the issue is already public, and any disclosure deadline you are
  working to.

## Process

Maintainers will:

1. acknowledge the report and confirm the private channel;
2. triage severity and reproduce the issue;
3. prepare a fix and a regression test;
4. release the fix, then publish a GitHub Security Advisory crediting the
   reporter unless anonymity is requested.

Timelines are communicated in the private report on a case-by-case basis, since
they depend on the severity, exploitability, and complexity of the fix.

## Scope

In scope:

- authentication and authorization bypasses;
- remote code execution, injection, and deserialization flaws;
- cryptographic misuse in the E2EE and federation code paths;
- server-side request forgery and federation trust-boundary violations;
- denial of service that is remotely triggerable by an unauthenticated peer;
- exposure of secrets, credentials, or private user data.

Out of scope:

- vulnerabilities in dependencies that already have an upstream advisory
  (report those upstream; we track them through the supply-chain gate);
- findings that require a pre-authenticated administrator account;
- missing hardening headers or best-practice suggestions with no demonstrated
  impact;
- social engineering of project maintainers or users.

## Coordination

We follow coordinated disclosure. Once a fix is available, the advisory is
published. Reporter credit is given by default; tell us in the report if you
prefer to stay anonymous.
