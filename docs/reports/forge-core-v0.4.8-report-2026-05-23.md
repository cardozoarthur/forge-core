# Forge Core v0.4.8 Report - 2026-05-23

## Objetivo

Implementar o primeiro contrato operacional de registry com `forge list`, avançando o caminho definido em v0.4.7 para listar workflows rodando e não rodando antes de expandir para `forge inspect`, subflows recursivos e reuso automático de flows.

## Mudanças

- Adicionado `forge list --output json`.
- Adicionado módulo `registry` para projetar workflows persistidos e runs associados sem alterar o estado do runtime.
- Adicionado `initial_goal` ao modelo de workflow para preservar o pedido inicial depois de mutações em runtime por `forge workflow update-goal`.
- Adicionados métodos de storage para carregar todos os workflows e todos os runs a partir do SQLite.
- Atualizados README e changelog para documentar o novo comportamento.

## Contrato exposto

Cada linha de registry expõe:

- `workflow_id`;
- `run_ids` e `run_statuses`;
- `initial_request`;
- `current_goal`;
- `workflow_status`;
- `lifecycle_state`;
- `running`;
- `workflow_revision`;
- `artifact_count`;
- `task_summary`;
- `created_at`.

## Lifecycle inicial

- Workflows com task em execução são projetados como `running`.
- Workflows bloqueados ou falhos preservam `blocked` ou `failed`.
- Workflows concluídos com todas as tasks completas são projetados como `scaled_to_zero`.
- Workflows ainda sem trabalho em execução são projetados como `idle`.

## Segurança

`forge list` é somente leitura. Ele deriva a visão do registry a partir do SQLite do Forge, sem usar CLIs instaladas como executores e sem tocar em Docker, Kubernetes ou Knative.

Registros antigos sem `initial_goal` continuam carregando; a projeção usa o goal original do run assíncrono quando disponível e, se não houver run, usa o goal atual como fallback.

## Validação executada

- `cargo test list_surfaces_workflow_registry_with_lifecycle_and_initial_request --test forge_cli_contract`: passou.
- `cargo fmt --check`: passou após aplicar `cargo fmt`.
- `cargo clippy --all-targets --all-features -- -D warnings`: passou.
- `cargo test`: passou com 32 testes de contrato.
- `cargo build --release`: passou.
- Smoke com binário release no `PATH`: `forge plan --goal "Create a delivery platform" --output json`: passou.
- Smoke com binário release no `PATH`: `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-run_b2eb69b815924e7fb39b55470da5575d`: passou.
- Smoke com binário release no `PATH`: `forge list --output json`: passou.

## Bloqueio operacional

`cargo install --path . --force` foi executado depois do build release, mas falhou porque `/home/arthur/.cargo` está somente leitura nesta sessão:

```text
failed to open: /home/arthur/.cargo/.crates.toml
Read-only file system (os error 30)
```

O código foi validado, mas o binário global do usuário não pôde ser atualizado a partir deste sandbox.

## Publicação

O contrato do GitHub CLI foi iniciado:

- `gh auth token` passou, com stdout redirecionado para não expor o token.
- `git remote get-url origin` retornou `https://github.com/cardozoarthur/forge-core.git`.

O commit/push não pôde ser concluído porque `git add` falhou ao tentar criar `.git/index.lock`:

```text
Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Read-only file system
```

As mudanças permanecem no worktree local e ainda não foram publicadas.

## Próximo ciclo recomendado

Implementar `forge inspect <workflow-id>` com renderização terminal do DAG, primeiro sem subflows, depois com `--verbose` para mostrar processos, subprocessos e subflows recursivos.
