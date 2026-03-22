# Kraken: Implement `listen` CLI Command

## Task
Implement the `mdrn listen` command that:
1. Parses stream address (hex) or stream_id
2. Reads chunks from stdin (CBOR, hex-encoded per line for testing)
3. Decrypts if encrypted
4. Decodes Opus to PCM
5. Outputs to audio device or WAV file

## Checkpoints
**Task:** Implement listen CLI command
**Started:** 2026-03-03T18:50:00Z
**Last Updated:** 2026-03-03T18:50:00Z

### Phase Status
- Phase 1 (Tests Written): VALIDATED (12 integration + 5 unit tests)
- Phase 2 (Implementation): VALIDATED (all tests passing)
- Phase 3 (Refactoring): VALIDATED (code clean, no warnings)

### Validation State
```json
{
  "test_count": 17,
  "tests_passing": 17,
  "tests_failing": 0,
  "files_modified": ["mdrn-cli/src/listen.rs", "mdrn-cli/src/main.rs", "mdrn-cli/tests/listen_command.rs"],
  "last_test_command": "cargo test --package mdrn-cli -- listen",
  "last_test_exit_code": 0
}
```

### Resume Context
- Current focus: COMPLETE
- Next action: None - implementation finished
- Blockers: None
