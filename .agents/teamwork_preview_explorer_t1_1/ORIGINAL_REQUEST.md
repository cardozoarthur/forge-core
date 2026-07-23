## 2026-07-04T10:38:58Z

Research the existing codebase at /home/arthur/projects/forge-core, especially tests/forge_cli_contract.rs, to understand how integration/E2E tests are implemented and how the forge binary is executed (e.g. assert_cmd).
Analyze the requirements for Feature 1 (CLI & Output Formatting) of the `forge teamwork` subcommand, which accepts --goal, --detached, and --output options, and supports JSON vs human-readable outputs.
Propose a complete design and concrete test cases for Feature 1, covering happy paths, boundary/corner cases, and error handling.
Write your detailed analysis and recommended test plan to /home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_1/handoff.md.
