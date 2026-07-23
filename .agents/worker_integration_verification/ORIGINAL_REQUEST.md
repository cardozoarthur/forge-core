## 2026-07-03T19:34:43Z
You are a worker with role 'Integration Verifier'.
Your task is:
1. Run `cargo test` and verify that the test suite passes cleanly.
2. Run `cargo clippy --all-targets --all-features -- -D warnings` to make sure there are no warnings or formatting errors.
3. Build `forge-core` in release mode using `cargo build --release`.
4. Run `./target/release/forge executors --output json` and verify that `antigravity` is in the returned list of executors/brains.
5. Create a handoff report at `/home/arthur/projects/forge-core/.agents/worker_integration_verification/handoff.md` summarizing:
   - Output of tests and clippy.
   - Command run and JSON output of `./target/release/forge executors --output json` verifying `antigravity` integration.
   - Verification that the skill file at `/home/arthur/.gemini/config/skills/forge/SKILL.md` is valid and correct.
Respond with send_message to the parent conversation when complete.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
