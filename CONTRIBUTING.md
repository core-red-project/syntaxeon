# Contributing to Syntaxeon

Contributions are welcome — bugs, fixes, features, or documentation.
This document covers how to work with the project as a contributor.

---

## Before You Start

- Search [existing issues](https://github.com/core-red-project/syntaxeon/issues) before opening a new one.
- For significant changes, open an issue first to discuss the direction before writing code.
- Read the [Code of Conduct](CODE_OF_CONDUCT.md). It applies to all interactions in this project.
- Changes to the language itself (grammar, rhyme rules, semantics) start in [spec/](spec/): the specification is authoritative and the implementation follows it. Record the decision in an ADR under `spec/adr/`.

---

## Reporting a Bug

Open a [GitHub Issue](https://github.com/core-red-project/syntaxeon/issues/new/choose) using the bug report template.

Include:
- What you expected to happen
- What actually happened
- Steps to reproduce
- Environment details (OS, runtime version, relevant config)

---

## Proposing a Feature

Open a [GitHub Issue](https://github.com/core-red-project/syntaxeon/issues/new/choose) using the feature request template, or submit a PR directly if the change is small and self-contained.

For larger features, an issue discussion first avoids wasted effort on both sides.

---

## Development Setup

```bash
just install   # toolchain components and git hooks
just check     # the full quality gate: format, lint, typecheck, tests, dependency audit
```

`just check` is what CI runs and what the pre-commit hook runs; it never modifies files. Run `just format` to apply formatting. Every recipe is listed by `just`.

---

## Workflow

1. Fork the repository and create a branch from `main`.
2. Name your branch descriptively — `fix/crash-on-empty-input`, `feat/offline-mode`.
3. Make your changes.
4. Open a pull request against `main` with a clear description of what changed and why.

---

## Pull Request Checklist

Before submitting:

- [ ] The project builds without errors
- [ ] Changes are described in [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`
- [ ] The PR description explains what changed and why
- [ ] New behavior is covered by tests where applicable

---

## Commit Style

This project uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). Every commit message must follow the format:

```
<type>: <description>

[optional body]
[optional footer]
```

Accepted types:

| Type       | Use for                                          |
|------------|--------------------------------------------------|
| `feat`     | New functionality                                |
| `fix`      | Bug fixes                                        |
| `docs`     | Documentation only                               |
| `style`    | Formatting, whitespace — no logic changes        |
| `refactor` | Code restructure without behavior change         |
| `test`     | Adding or updating tests                         |
| `chore`    | Build process, tooling, dependencies             |
| `perf`     | Performance improvements                         |

Examples:

```
feat: add offline fallback for config reads
fix: prevent crash when scripts directory is missing
docs: update installation steps for cross-compilation
chore: bump dependencies to latest stable
```

Commits that don't follow this format will be flagged during review.

---

## Questions

If something in the codebase is unclear, open an issue with the `question` label before assuming it's a bug.

---

*Syntaxeon is A CoreRed Project. Part of the [Sxnnyside Project](https://sxnnysideproject.com).*
