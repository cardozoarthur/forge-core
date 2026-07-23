## 2026-07-03T21:42:47Z

You are the Worker for Milestone 2: Telegram Notification Delivery (R3).
Your working directory is `/home/arthur/projects/forge-core/.agents/worker_telegram_delivery_m2`.

Please perform the following tasks:
1. Create a new workflow to get a valid `workflow_id`. For example, run:
   `./target/release/forge plan --goal "Deliver strategic report to Telegram" --output json` (or request start).
2. Retrieve the `workflow_id` from the output JSON.
3. Trigger Telegram delivery in simulated mode:
   - Command: run `./target/release/forge event emit`
   - Set environment variables: `FORGE_TELEGRAM_EGRESS_MODE=simulate` and `TELEGRAM_BOT_TOKEN=mock_token` (and `TELEGRAM_CHAT_ID=12345` if needed).
   - CLI options:
     - `--adapter telegram.bot_send_document`
     - `--event-type telegram.report`
     - `--action send_report`
     - `--payload` containing `workflow_id`, `chat_id` (e.g. "12345"), and the path to the strategic document `/home/arthur/projects/forge-core/forge_strategic_report.md`.
4. Verify that the `telegram_delivery_record` artifact is created and attached to the workflow (e.g. by checking if the artifact is listed in `./target/release/forge artifacts --workflow <workflow_id>` or similar listing commands).
5. Document all commands run, their outputs, and verify the delivery record is successfully attached.
6. Write a handoff.md in your working directory and send a message back to the parent (conversation ID: 3e9f825f-a52f-4f9b-8826-e0ccd6f322a6) with the path to your handoff.md.

MANDATORY INTEGRITY WARNING: DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
