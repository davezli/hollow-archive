# Fixtures

Recordings of real logins (`<ver>-<region>-<desc>.pcapng`) and, optionally, the
reference tool's export of the same account (`<same>.expect.json`). Both are
gitignored because they contain a real inventory and login token. Record with
`pktmon` per `specs/implementation.md` section 3.4 and drop the files here; the
`replay` integration test picks them up automatically and is skipped when none
are present.
