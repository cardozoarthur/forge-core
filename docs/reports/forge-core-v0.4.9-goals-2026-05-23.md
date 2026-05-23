# Forge Core v0.4.9 Goals Report - 2026-05-23

## Objetivo

Adicionar três goals estruturais ao Forge e corrigir compatibilidade do `forge list` com workflows antigos.

## Goals adicionados

### Context Routing Engine

Forge deve fornecer contexto mínimo correto para cada executor.

Capacidades alvo:

- comprimir contexto grande;
- resumir histórico e artifacts;
- selecionar somente arquivos, decisões e restrições relevantes;
- versionar context packets;
- shardear contexto por workflow, subflow, task e validation gate;
- reduzir custo e reasoning redundante.

### Deterministic + AI Hybrid Graph

Forge deve misturar no mesmo grafo:

- AI tasks;
- deterministic code tasks;
- Python/Node.js code nodes para lógica frequente ou estável;
- waits;
- cron;
- approvals;
- validation;
- rollback;
- deployment;
- notificações e cost reports.

Forge deve decidir quando uma tarefa não precisa de IA e pode ser executada como código local.

### Long-running Cognition

Forge deve tratar cognição como execução durável:

- pause/resume;
- async continuation;
- durable execution;
- checkpointing;
- partial retry;
- resumable context.

## Correção aplicada

`forge list` agora consegue carregar workflows antigos sem `async_policy`, usando a política padrão `sync/inline`.

## Próximo passo recomendado

Implementar `forge inspect <workflow-id>` como próximo incremento, mantendo esses novos goals no prompt de autoevolução para orientar ciclos seguintes.
