# Kraken Implementation: Vouch Command

## Task
Implement the vouch command for MDRN CLI - final Phase 1 command for trust network.

## Checkpoints
<!-- Resumable state for kraken agent -->
**Task:** Implement vouch command with TDD
**Started:** 2026-03-22T17:30:00Z
**Last Updated:** 2026-03-22T17:45:00Z

### Phase Status
- Phase 1 (Tests Written): VALIDATED (4 tests exist, all failing as expected)
- Phase 2 (Implementation): VALIDATED (all 4 tests passing)
- Phase 3 (Refactoring): VALIDATED (tracing moved to stderr)
- Phase 4 (Documentation): VALIDATED (output report written)

### Validation State
```json
{
  "test_count": 4,
  "tests_passing": 4,
  "tests_failing": 0,
  "files_modified": ["mdrn-cli/src/main.rs"],
  "last_test_command": "cargo test --package mdrn-cli test_vouch",
  "last_test_exit_code": 0
}
```

### Resume Context
- Current focus: COMPLETE
- Next action: N/A
- Blockers: None

## Requirements
1. Parse subject hex to Identity
2. Load keypair from file
3. Create signed Vouch with optional expiration
4. Output CBOR to stdout
5. Fail gracefully for invalid hex or missing keypair

## Test Expectations (from cli_commands.rs)
- test_vouch_creates_credential: outputs CBOR to stdout, deserializes and verifies
- test_vouch_with_expiration: --expires 30 sets expires_at ~30 days from now
- test_vouch_invalid_subject_hex: fails with invalid hex input
- test_vouch_missing_keypair_file: fails with non-existent keypair path

## Implementation Summary
1. Replaced vouch stub in main.rs (lines 407-460)
2. Fixed tracing to output to stderr (line 129) to keep stdout clean for CBOR
3. All 4 vouch tests pass
4. 5 mdrn-core identity tests pass
