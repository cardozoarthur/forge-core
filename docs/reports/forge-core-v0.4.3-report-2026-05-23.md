# Forge Core v0.4.3 Report - 2026-05-23

## Objetivo

Fazer `forge request status` refletir o estado atual do workflow associado ao `run_id`, em vez de expor apenas o registro inicial da execução assíncrona.

## Implementado

- Novo relatório tipado de status para requests assíncronos.
- `forge request status --run <run-id>` agora carrega o workflow atual associado ao run.
- O JSON de status passa a expor:
  - `goal` como objetivo atual do workflow;
  - `requested_goal` como objetivo original do request;
  - `workflow_status`;
  - `workflow_revision`;
  - `artifact_count`;
  - `task_summary` com contagem por estado.
- Teste de contrato cobrindo mutação de objetivo e anexo de artefato antes da consulta por `run_id`.

## Contrato operacional

O `run_id` continua sendo o identificador estável entregue a Codex/OpenCode e skills. Ao consultar esse identificador, Forge projeta o estado atual do workflow persistido, preservando a intenção original em `requested_goal`.

Isso evita que um executor ou skill tome decisões com base em um snapshot antigo quando o objetivo ou os artefatos foram alterados por `forge workflow update-goal` ou `forge workflow attach-artifact`.

## Validação executada

O contrato novo foi coberto primeiro por:

```bash
cargo test request_status_reflects_current_workflow_mutations_for_async_callers --test forge_cli_contract
```

A validação completa desta versão foi executada com:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Resultado:

- `cargo fmt --check`: passou.
- `cargo clippy --all-targets --all-features -- -D warnings`: passou.
- `cargo test`: passou, com 29 testes de contrato.
- `cargo build --release`: passou.

Smokes CLI executados com o binário release:

```bash
./target/release/forge --store /tmp/forge-core-v043-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json
./target/release/forge --store /tmp/forge-core-v043-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v043
```

## Instalação local

`cargo install --path . --force` foi tentado após a validação, mas o sandbox bloqueou escrita em `/home/arthur/.cargo/.crates.toml` com `Read-only file system (os error 30)`.

Como fallback dentro das raízes graváveis deste ciclo, a versão validada foi instalada em:

```bash
cargo install --path . --force --root /home/arthur/projects/forge-core/.forge/local-install --offline
```

Resultado:

- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version`: `forge 0.4.3`.
- `forge --version` no PATH global permanece `forge 0.4.2` porque a instalação global em `/home/arthur/.cargo` não é gravável neste sandbox.

## Publicação

O commit/push validado foi bloqueado pelo sandbox: `.git` está montado como somente leitura e `git add` falhou ao criar `.git/index.lock` com `Sistema de ficheiros só de leitura`.

As alterações permanecem aplicadas no working tree, mas não foi possível criar o commit nem executar `git push` sem acesso de escrita ao metadata do repositório.

## Próximo passo

Propagar mudanças reais de execução para o run record, incluindo estados `running`, `blocked`, `completed` e razão de rework por task.
