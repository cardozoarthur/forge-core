# Forge Core v0.4.0 Report - 2026-05-23

## Objetivo

Ajustar Forge para o fluxo ideal de uso como skill: Codex/OpenCode fazem um pedido curto, recebem um `run_id`, e o restante acontece de forma assíncrona dentro do Forge.

## Implementado

- `forge request start` cria workflow e run assíncrono.
- `forge request status` consulta o run depois.
- `run_id` é separado de `workflow_id`.
- `forge self run` cria workflow/run de auto-evolução do Forge.
- Cada ciclo de auto-evolução gera prompt e report artifact.
- `--until` é obrigatório no self-run e rejeita datas no passado.
- `--executor codex --executor opencode` define os executores autorizados para alternância.
- `--dry-run` permite planejar sem chamar executores reais.
- `--push` torna push explícito; sem ele, o Forge não publica commits.

## Contrato operacional

Quando usado como skill:

```bash
forge request start --goal "<pedido>" --origin codex --output json
```

O chamador deve retornar o `run_id`. A continuidade deve ser feita por:

```bash
forge request status --run <run-id> --output json
```

## Próximo passo

Executar a primeira rodada real de `forge self run` com Codex/OpenCode, acompanhar o primeiro ciclo e depois deixar rodando em tmux até segunda-feira às 10:00.
