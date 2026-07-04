---
name: forge-core-agent
description: Forge Core agent configuration, registering brain/soul profiles, executor policies, and adapter credentials.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Agent and Executor Contract

Forge controls agent execution through bounded executor adapters, brain/soul profiles, and authorization policies. No agent may execute tasks without valid profile registration and policy checks.

## Configuring and Registering Brain/Soul Profiles

A Brain/Soul profile defines the cognitive model configuration, temperature, system instructions, and capabilities of an executor.

To configure and register a profile:
1. Define a JSON configuration file representing the profile (e.g., `soul_profile.json`):
   ```json
   {
     "profile_id": "agy-coder",
     "model_provider": "antigravity",
     "model_name": "agy-default",
     "temperature": 0.2,
     "max_tokens": 8192,
     "system_instruction": "You are a professional Rust developer adhering to strict safety and performance constraints."
   }
   ```
2. Register the profile via `forge`:
   ```bash
   forge executor register-profile --profile-path ./soul_profile.json --output json
   ```

## Configuring Executor Options

Executor options specify execution policies such as concurrency, timeouts, and local directory structures:

- **Local executor policy**: Mark local CLI tools (e.g. `agy`) as allowed or disallowed.
- **Async run substrates**: Detect and execute in Docker, Kubernetes, or Knative. Remember that these are async run substrates, not cognitive executors. Do not mutate or install Knative/Kubernetes without explicit user authorization.

CLI commands:
```bash
forge executor policy --list --output json
forge executor policy --allow "agy" --output json
```

## Adapter Credentials and Quotas

Executor adapters require secure credential binding (API keys, workspace tokens, API endpoints) and strictly follow quota limits.

### Credential Binding
Credentials are bound using the environment or the SQLite state store. Do not write credentials in plaintext in files.
```bash
forge executor credentials set --adapter "agy" --key "ANTIGRAVITY_API_KEY" --value "secret_value" --output json
```

### Quotas and Limits (`ai-limits`)
Forge monitors usage metrics (tokens, costs, execution duration) against the configured policies.
```bash
forge executor limits --profile "agy-coder" --token-limit 500000 --cost-limit-usd 2.50 --output json
```
If an executor hits a limit, Forge throws an execution exception and triggers the fallback executor routing flow.
