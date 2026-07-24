## 2026-07-03T19:37:40Z
You are a worker with role 'Telegram Delivery Agent'.
Your task is:
1. Run `./target/release/forge plan --goal "Analyze Forge ecosystem and deliver strategic report" --output json` to plan a new workflow and extract the `workflow_id`.
2. Construct the delivery payload:
   ```json
   {
     "workflow_id": "<workflow_id>",
     "document_path": "/home/arthur/projects/forge-core/forge_strategic_report.md",
     "caption": "Forge Ecosystem Strategic Report"
   }
   ```
3. Run the event emission command with the environment variables set:
   - Env:
     - `TELEGRAM_BOT_TOKEN="123456:ABC-DEF1234"`
     - `TELEGRAM_CHAT_ID="123456"`
     - `FORGE_TELEGRAM_EGRESS_MODE="simulate"`
   - Command:
     `./target/release/forge events emit --addon forge.addon.notification --adapter telegram.bot_send_document --event-type telegram.report --action send_final_report --origin codex --payload '<payload>' --output json`
4. Run `./target/release/forge inspect <workflow_id> --verbose --output json` to verify that the `telegram_delivery_record` or `event_egress_delivery` is successfully registered/attached as evidence.
5. Create a handoff report at `/home/arthur/projects/forge-core/.agents/worker_telegram_delivery/handoff.md` summarizing:
   - The generated workflow ID.
   - The JSON response from `forge events emit`.
   - The verification output from `forge inspect <workflow_id>` proving the delivery record artifact exists.
Respond with send_message to the parent conversation when complete.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
