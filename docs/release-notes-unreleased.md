# Release Notes — Unreleased (since v0.4.1)

**Date:** Pending
**Status:** Unreleased

## Summary

Two changes have landed on `main` since the v0.4.1 release:

1. **Golden-path CLI integration tests** — New mocked HTTP test coverage for lens management, custom bangs, redirects, and saved-assistant selection commands.
2. **CI concurrency groups** — All four CI workflows now use `concurrency` groups to cancel redundant in-flight runs when a new push arrives on the same branch.

## Changes

### Testing

- Added golden-path integration tests for CLI commands with mocked HTTP responses, covering lens, bangs, redirects, and saved-assistant flows. (faa7346)

### CI/CD

- Added concurrency groups to all four GitHub Actions workflows, reducing wasted CI minutes by auto-cancelling superseded runs. (c03d6ea)

---

*These notes were drafted by [Nightshift](https://github.com/nightshift-micr) from `git log v0.4.1..HEAD`.*
