# CodeQL test traffic findings

CodeQL's `rust/cleartext-transmission` query flags HTTP requests in
`crates/*/tests/` where a synthetic UUID session id is interpolated into a
loopback URL. Those are integration tests against `127.0.0.1`: there is no
production network path and no sensitive data. Callcraft's production server is
loopback-only by design, so loopback HTTP is the intended transport.

Inline suppression is not available for Rust CodeQL (the `AlertSuppression`
query is missing), and leaving the alerts open makes the CodeQL check fail on
every push. They are dismissed as **used in tests** and this file is the durable
record. Production code remains scanned.

If a new test adds a similar loopback request, dismiss the resulting alert with
the same reason and update this note if the rationale changes.
