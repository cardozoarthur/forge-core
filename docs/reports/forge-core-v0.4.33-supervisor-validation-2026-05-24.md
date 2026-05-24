# Forge Core v0.4.33 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.33`.
- Commit promovido pelo workflow: `049121b`.
- Estado do GitHub após promoção: `HEAD -> main, origin/main`.
- O ciclo `run_9a3040f34bbb4d7f858112fd4ec9c758` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora projeta a identidade de cache do Context Routing Engine em superfícies operacionais compactas:

- `forge task handoff` passou a emitir `forge.executor_handoff.v2`;
- o handoff inclui `context_routing_cache_key`, `context_routing_fingerprint_schema_version` e `context_routing_lineage_sha256`;
- `forge inspect --output json` inclui `routing_cache_key`, `routing_fingerprint_schema_version` e `routing_lineage_sha256` em `nodes[].context_route`;
- o diagrama terminal mostra um hash curto da rota de contexto.

Isso permite comparar handoff, inspect e context sem abrir o pacote completo.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Expose routing cache keys for handoff and inspect"
forge task handoff --workflow <workflow_id> --task <first_task> --executor codex
forge inspect <workflow_id> --output json
```

Resultado:

```txt
handoff_schema=forge.executor_handoff.v2
handoff_context_routing_cache_key_length=64
handoff_context_routing_fingerprint_schema=forge.context.routing_fingerprint.v1
handoff_context_routing_lineage_sha256_length=64
inspect_routing_cache_key_length=64
inspect_routing_fingerprint_schema=forge.context.routing_fingerprint.v1
terminal_diagram_cache_hash=true
```

O comportamento esperado foi confirmado: o adapter bounded recebe a chave de cache no envelope de handoff, e o operador vê a mesma identidade de roteamento no `inspect`.

## Nota sobre o relatório interno

O relatório interno registra bloqueios de instalação global e push observados de dentro do sandbox do Codex. Fora desse sandbox, o wrapper do Forge concluiu instalação, commit e push.

Estado verdadeiro verificado pelo supervisor:

```txt
forge --version = forge 0.4.33
git log -1 = 049121b (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Usar o routing cache key para decisões explícitas de checkpoint freshness e partial retry em `forge task handoff`, permitindo que adapters saibam quando devem retomar, atualizar contexto ou reexecutar uma task.

O goal de Personality/Soul Routing continua ativo. A base atual já preserva persona no contexto, no lineage e na validação; uma evolução útil é projetar essa identidade de persona também no handoff/artifact pipeline para trabalhos de escrita humana.
