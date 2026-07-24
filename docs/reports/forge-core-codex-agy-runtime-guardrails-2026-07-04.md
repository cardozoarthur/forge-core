# Forge Core: guardrails nativos sem bloquear Codex e agy

Data: 2026-07-04

## Resumo

O objetivo do anexo era transformar os 10 guardrails de segurança em capacidades nativas do runtime, não em funcionalidades opcionais de addons. Também havia um critério funcional explícito: Forge precisa continuar conseguindo usar Codex e `agy`; se a política impedir esses executores autorizados, o runtime não está funcional.

Esta entrega cobre os dois pontos:

- Codex e `agy` autorizados continuam usáveis via harness Forge-first.
- `harness exec` e `run --simulate` agora carregam o mesmo bundle nativo de 10 guardrails antes da execução.

## O que mudou no código

- Novo módulo `src/security.rs` com o schema `forge.runtime.security_guardrails.v1`.
- `CliHarnessExecReceipt` agora inclui `runtime_security_guardrails`.
- `ExecutionReport` agora inclui `runtime_security_guardrails`.
- `src/executor.rs` centraliza prontidão de executor em `executor_has_authorized_runtime_path`.
- `usable`, integrações, candidatos de quota e status de brain usam a mesma regra de execução autorizada.

## Os 10 guardrails nativos

O runtime agora expõe estes controles como `native_runtime_capability = true`:

1. `filesystem_permissions`
2. `command_execution_permissions`
3. `network_permissions`
4. `credential_secret_permissions`
5. `tool_usage_permissions`
6. `resource_consumption_limits`
7. `human_approval_gates`
8. `tenant_project_isolation`
9. `audit_traceability`
10. `organizational_policy_engine`

O bundle declara `enforcement_owner = forge_runtime` e `coverage_scope = workflow_agent_cli_mcp_deterministic_process`.

## Codex e agy

Forge agora considera um executor funcional quando ele está:

- instalado;
- configurado;
- autorizado pela política local;
- pronto por smoke não interativo direto ou por harness Forge-first com entrypoint auditável.

Isso evita o erro de tratar CLIs interativos como inutilizáveis mesmo quando Forge já possui um caminho seguro e auditável para executá-los.

## Evidência de TDD

Testes adicionados/ajustados:

```bash
rtk cargo test forge_first_ready_codex_and_agy_remain_usable_brains
rtk cargo test harness_token_headroom_compresses_logs_and_mcp_wrap_plan_shapes_cli_environment
rtk cargo test parallel_execution_reports_concurrent_wave_metrics
```

Cada regressão falhou primeiro pelo campo/comportamento ausente e passou após a implementação.

## Validação obrigatória

```bash
rtk cargo fmt --check
rtk proxy cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test
rtk cargo build --release
```

Resultados:

- `cargo fmt --check`: passou.
- `cargo clippy --all-targets --all-features -- -D warnings`: passou via `rtk proxy`.
- `cargo test`: 680 testes passaram.
- `cargo build --release`: passou.

## Smokes

Smokes obrigatórios:

```bash
target/release/forge --store /tmp/forge-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json
target/release/forge --store /tmp/forge-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke
```

Ambos passaram.

Smoke de workflow runtime:

```json
{"decision":"allowed_forge_first_exec","guardrail_count":10,"status":"completed"}
```

Smoke de `harness exec`:

```json
{"decision":"allowed_dry_run","guardrail_count":10,"status":"harness_exec_dry_run"}
```

Smoke Codex + `agy`:

```json
{"selected_brain":"codex","usable":["agy","codex"]}
```

Checagem Forge do próprio objetivo:

```json
{"status":"passed","workflow_id":"wf_3b9c432085884c5fbdc5f4f5f08fbff3"}
```

Também foi verificado que:

- `codex_primary_brain.enabled = true`;
- `agy_codex_bridge.enabled = true`;
- candidatos de quota de `codex` e `agy` ficaram `eligible`;
- brains de `codex` e `agy` ficaram `ready`.

## Estado

Implementado e validado localmente. A entrega agora cobre runtime guardrails nativos e execução funcional de Codex/`agy` sob política Forge-first.
