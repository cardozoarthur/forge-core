# Forge Core v0.4.6 Report - 2026-05-23

## Objetivo

Expor a última evidência de validação de autoevolução diretamente em `forge request status`, sem transformar o run record em uma cópia paralela do estado do workflow.

## Implementado

- Adicionado o campo `latest_validation_evidence` em `forge request status --output json`.
- O resumo é derivado do artifact persistido `self-evolution-cycle-NNN-validation.json`.
- O status inclui caminho do artifact, SHA-256, versão do schema, versão do prompt packet, ciclo, executor, status de validação e contagem de comandos por estado.
- Quando o workflow não possui artifact de validação, o campo permanece `null`.

## Impacto

Chamadores assíncronos de Codex/OpenCode conseguem consultar o `run_id` e enxergar rapidamente a evidência mais recente que sustenta ou bloqueia a promoção, sem precisar listar artifacts e parsear arquivos manualmente.

## Validação

Comandos obrigatórios executados neste ciclo:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Resultado: todos passaram.

## Instalação local

- `cargo install --path . --force` foi executado, mas falhou porque o sandbox não permite escrita em `/home/arthur/.cargo/.crates.toml` (`Read-only file system`).
- A instalação local gravável do checkout foi atualizada com:

```bash
CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline
```

- O binário em `.forge/local-install/bin/forge` reporta `forge 0.4.6`.

## Publicação

- `gh auth token` foi validado sem expor o token.
- `git remote get-url origin` retornou `https://github.com/cardozoarthur/forge-core.git`.
- A criação do commit foi bloqueada porque o sandbox não permite escrita em `.git/index.lock` (`Read-only file system`).
- Por isso, o commit e o `git push` não foram concluídos neste ambiente.

## Próximo passo

Adicionar um contrato de resposta de executor com traces JSONL e validação de schema, usando leases já persistidos para controlar posse de tarefas longas.
