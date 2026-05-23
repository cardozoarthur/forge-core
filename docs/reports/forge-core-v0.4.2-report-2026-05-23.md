# Forge Core v0.4.2 Report - 2026-05-23

## Objetivo

Adicionar um contrato mínimo de leases de task para impedir que dois executores adquiram o mesmo trabalho assíncrono ao mesmo tempo.

## Implementado

- Novo comando `forge task acquire`.
- Novo comando `forge task release`.
- Nova tabela SQLite `task_leases`, com chave primária por `workflow_id` e `task_id`.
- Relatório JSON estruturado para aquisição, conflito e liberação de lease.
- Eventos persistidos para `task_lease_acquired`, `task_lease_conflict`, `task_lease_released` e `task_lease_release_failed`.
- Campos explícitos no relatório de auto-evolução para instalação local pós-validação e publicação via GitHub CLI.
- Execução pós-validação em ciclos não dry-run:
  - `cargo install --path . --force`;
  - `gh auth status`;
  - `gh repo view --json url,visibility`;
  - `git push` com timeout quando `--push` foi solicitado e o repositório é público.

## Contrato operacional

Um executor só recebe ownership temporário de uma task quando Forge persiste o lease. Enquanto o lease estiver ativo, outro executor recebe `lease_conflict` com o lease atual e o comando sai com código diferente de zero.

Leases expirados podem ser substituídos por uma nova aquisição, mantendo Forge como fonte de verdade para coordenação entre Codex, OpenCode e futuros adapters.

Durante a validação completa, também foi corrigido o contrato já existente de auto-evolução: o prompt agora declara a atualização local com `cargo install --path . --force` e a publicação por GitHub CLI, o `SelfCycleReport` expõe esses comandos de forma auditável e ciclos não dry-run executam essa etapa somente depois da validação passar.

## Validação executada

O contrato é coberto pelo teste:

```bash
cargo test task_lease_prevents_two_executors_from_acquiring_same_task
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
- `cargo test`: passou, com 28 testes de contrato.
- `cargo build --release`: passou.

Smokes CLI executados com o binário release:

```bash
./target/release/forge --store /tmp/forge-core-v042-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json
./target/release/forge --store /tmp/forge-core-v042-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v042
```

## Próximo passo

Conectar leases ao executor runtime real para aquisição automática antes de executar uma task e liberação explícita após validação ou rework.
