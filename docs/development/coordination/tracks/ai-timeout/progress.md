# ai-timeout — FR-15 Phase 3.2

Status: READY_FOR_TEST
Worktree: .worktrees/datazen-ai-timeout
Branch: feature/ai-timeout
HEAD: 3672d7608

Done: map_http_error 400/429; RetryConfig/retry_with_backoff/compute_backoff_delay/sanitize_400_error/parse_retry_after verified; openai_chat/responses/anthropic retry integrated; ai-api types max_timeout_secs confirmed; CARGO_TARGET_DIR=target/cargo-timeout test ok

Next: Tester verifies retry in complete() paths; mark PASSED.
