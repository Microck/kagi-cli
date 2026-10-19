
## Deterministic documentation browser coverage

The pinned tester-army/e2e suite opens every source documentation page and
checks its rendered title. Run the production build, install Chromium with
`e2e-web install chromium`, then run the package’s `test:e2e` script.
It makes no model calls, uses no account credentials, and does not replace
the project’s native/CLI integration tests. Browser traces remain in `.e2e/`.
