
## Deterministic documentation browser coverage

The pinned tester-army/e2e suite opens every source documentation page and
checks its rendered title. Run the production build, install Chromium with
`pnpm exec e2e-web install chromium`, then run the package’s `test:e2e` script.
It makes no model calls, uses no account credentials, and does not replace
the project’s native/CLI integration tests. Browser traces remain in `.e2e/`.

CI runs the full browser suite on every pull request and on pushes to `main`.
New runs cancel older runs for the same ref, so superseded revisions do not
consume runners while the latest revision waits. Different PRs and `main`
remain independent.
