# Forge Core v0.4.30 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.30`.
- Commit promovido pelo workflow: `ad6f439`.
- Estado do GitHub após promoção: `HEAD -> main, origin/main`.
- O ciclo `run_1e15acaa67b94461ae48335a684f451d` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora expõe readiness de handoff no registry:

- `forge list --output json` inclui `summary.context_handoff`;
- cada workflow listado inclui `context_handoff`;
- o schema é `forge.registry_context_handoff.v1`;
- os contadores indicam total de tasks, tasks prontas, tasks bloqueadas, bloqueio por contexto ausente e bloqueio por dependências.

A versão também atualizou o Context Routing Engine:

- `forge context` passou para `forge.context.v14`;
- shards obrigatórios são selecionados antes dos opcionais;
- isso evita que contexto opcional consuma orçamento antes de seções essenciais para execução.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Build registry-level context handoff routing visibility"
forge list --output json
```

Resultado global:

```txt
schema=forge.registry_context_handoff.v1
total_tasks=8
ready_tasks=1
blocked_dependencies=7
```

Resultado na linha do workflow:

```json
{
  "workflow_status": "pending",
  "lifecycle_state": "idle",
  "running": false,
  "context_handoff": {
    "schema_version": "forge.registry_context_handoff.v1",
    "total_tasks": 8,
    "ready_tasks": 1,
    "blocked_tasks": 7,
    "blocked_missing_context": 0,
    "blocked_dependencies": 7,
    "blocked_missing_context_and_dependencies": 0
  }
}
```

O comportamento esperado foi confirmado: o `list` serve como visão operacional barata para saber quais workflows estão parados/ociosos e quais tasks estão prontas ou bloqueadas antes de abrir `inspect` ou emitir `task handoff`.

## Nota sobre o relatório interno

O relatório interno registra bloqueios de instalação global e push observados de dentro do sandbox do Codex. Fora desse sandbox, o wrapper do Forge concluiu instalação, commit e push.

Estado verdadeiro verificado pelo supervisor:

```txt
forge --version = forge 0.4.30
git log -1 = ad6f439 (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Adicionar fatias de checkpoint freshness no registry e no `forge inspect`, permitindo distinguir tasks prontas, bloqueadas por dependência, bloqueadas por contexto e bloqueadas por checkpoint/resume obsoleto antes de entregar trabalho a um executor.

O goal de Personality/Soul Routing continua ativo no pacote de autoevolução. A base atual já tem persona por node, source models de Codex/Paperclip, hash em lineage, contexto roteado e validação de promoção.
