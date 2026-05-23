# Forge Core Self-Evolution First Run - 2026-05-23

## Status

O Forge Core está operacional como runtime de autoevolução controlada.

- Sessão em execução: `tmux forge-self-evolve`.
- Stop date configurado: `2026-05-25T10:00:00-03:00`.
- Executor usado no primeiro ciclo acompanhado: `codex`.
- Repositório público atualizado: `https://github.com/cardozoarthur/forge-core`.
- Versão instalada globalmente após validação: `forge 0.4.4`.

## Primeiro ciclo acompanhado

- `run_id`: `run_4098c2eb84c94bef96da3b62c366a43d`.
- `workflow_id`: `wf_f935687771a94412844edc341abb7b23`.
- Prompt packet: `forge.self_evolution.prompt.v1`.
- Prompt SHA-256: `635b3871700f0620d9e864727cd6c20b0241f2fe411c744aea7da5ebf405cd53`.
- Resultado do ciclo: `executor_completed`.
- Validação: passou.
- Autoatualização local: passou.
- Publicação do projeto: passou.
- Commit publicado pelo ciclo: `2f60b1c`.

## Evolução entregue hoje

- `0.4.0`: skill-style async handoff com `forge request start/status`, retornando `run_id`.
- `0.4.1`: prompt packets versionados e auditáveis com SHA-256.
- `0.4.2`: leases persistidos de task para impedir concorrência entre executores no mesmo trabalho.
- `0.4.3`: `forge request status` agora reflete o estado atual do workflow associado ao `run_id`, incluindo goal atual, goal original, revisão, artifacts e resumo de tasks.
- `0.4.4`: stdout JSON do `forge self run --output json` foi protegido contra logs de validação, mantendo artifacts parseáveis.

## Validação

Gate completo executado na 0.4.4:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Resultado: 29 testes passaram, build release passou e `forge --version` retorna `forge 0.4.4`.

## Observações operacionais

- `gh auth token` funciona e é usado como gate local de credencial.
- `api.github.com` não respondeu nesta máquina durante os testes; por isso o Forge usa `gh auth token` + `git remote get-url origin` + `git push` com timeout em vez de depender de `gh repo view`.
- O JSONL inicial do loop foi rotacionado para `.forge/self-evolve-loop.mixed-20260523T182746-0300.log` porque continha logs de validação misturados. Os próximos ciclos usam `.forge/self-evolve-loop.jsonl` limpo.

## Próximo foco

O próximo ciclo recomendado é propagar estados reais de execução para run records, incluindo `running`, `blocked`, `completed` e razões de rework por task.
