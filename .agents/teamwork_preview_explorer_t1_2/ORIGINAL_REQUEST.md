## 2026-07-04T10:38:58Z

Explore the codebase to understand the task graph decomposition, mapping rules, and web benchmark retrieval/ranking (LMSYS, HumanEval, MMLU) as described in PROJECT.md under Feature 2.
Analyze how this roster heuristics and benchmark ranking logic can be tested in an opaque-box, requirement-driven E2E fashion (e.g., how the CLI chooses different brains or decomposes different goals, what benchmark data we expect, how SQLite caching is accessed or bypassed).
Design concrete E2E test cases for Feature 2, including happy paths, boundaries, and combinations. Propose mock benchmark data setup or SQLite database states for E2E verification.
Write your detailed analysis and recommended test plan to /home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_2/handoff.md.
