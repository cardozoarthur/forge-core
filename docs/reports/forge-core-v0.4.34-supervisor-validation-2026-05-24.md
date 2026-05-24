# Forge Core v0.4.34 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.34`.
- Commit promovido pelo workflow: `75ba0d3`.
- Estado do GitHub após promoção: `HEAD -> main, origin/main`.
- O ciclo `run_e442e80cda3f4fb4b6962b55ff139a7c` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora usa a chave de roteamento de contexto para planejar retomada no handoff.

Principais mudanças:

- `forge task checkpoint` aceita `--context-routing-cache-key`;
- `forge task handoff` passou para `forge.executor_handoff.v3`;
- o pacote de handoff inclui `resume_plan`;
- o `resume_plan` informa checkpoint, checksum antigo, routing cache key antigo, routing cache key atual, status, ação recomendada, flag de partial retry e motivo.

As ações possíveis incluem:

- `start_fresh`;
- `refresh_context_before_resume`;
- `resume_from_checkpoint`;
- `partial_retry_with_fresh_context`.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Handoff resumable context route"
forge task handoff --executor codex
forge task checkpoint --context-routing-cache-key <first_route_key>
forge task release
forge task handoff --executor codex
```

Resultado:

```txt
handoff_schema=forge.executor_handoff.v3
resume_context_status=checkpoint_current
resume_plan.status=checkpoint_route_changed
resume_plan.action=partial_retry_with_fresh_context
partial_retry_recommended=true
checkpoint_id_matches=true
checkpoint_route_key_length=64
current_route_key_length=64
```

O comportamento esperado foi confirmado: o Forge compara a rota do checkpoint com a rota atual e orienta o executor a fazer retry parcial com contexto fresco quando elas divergem.

## Nota sobre o relatório interno

O relatório interno registra bloqueios de instalação global e push observados de dentro do sandbox do Codex. Fora desse sandbox, o wrapper do Forge concluiu instalação, commit e push.

Estado verdadeiro verificado pelo supervisor:

```txt
forge --version = forge 0.4.34
git log -1 = 75ba0d3 (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Projetar o `resume_plan` em `forge inspect --output json` e no diagrama terminal, para que operadores vejam pressão de partial retry antes de emitir um handoff específico.

Depois disso, uma evolução relevante é projetar Personality/Soul Routing no handoff/artifact pipeline, usando a mesma abordagem de identidade auditável já aplicada ao contexto.
