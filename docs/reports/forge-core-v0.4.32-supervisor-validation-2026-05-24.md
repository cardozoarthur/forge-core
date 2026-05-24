# Forge Core v0.4.32 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.32`.
- Commit promovido pelo workflow: `a804211`.
- Estado do GitHub após promoção: `HEAD -> main, origin/main`.
- O ciclo `run_e1adbe7c1795401b83e2e5617b696867` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora inclui `routing_fingerprint` dentro de `forge context --output json`.

Esse fingerprint é um contrato versionado para cache e invalidação de contexto:

- schema `forge.context.routing_fingerprint.v1`;
- `cache_key` estável;
- revision do workflow;
- profile do executor;
- `context_sha256`;
- `lineage_sha256`;
- componentes hashados de policy, profile, lineage, budget, seções selecionadas/omitidas, contexto obrigatório ausente, dependências, subflows, resume state e payload.

Isso melhora o Context Routing Engine porque um executor ou adapter pode comparar um cache key em vez de comparar o pacote completo de contexto.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Build fingerprinted context routing for repeated executor handoffs"
forge context --workflow <workflow_id> --task <extract_requirements> --budget 1100
forge context --workflow <workflow_id> --task <extract_requirements> --budget 1100
forge workflow update-goal --workflow <workflow_id> --goal "Build fingerprinted context routing after a goal mutation"
forge context --workflow <workflow_id> --task <extract_requirements> --budget 1100
```

Resultado:

```txt
schema=forge.context.routing_fingerprint.v1
executor_profile=ai_reasoning
cache_key_length=64
component_count=11
same_cache_key_before_mutation=true
cache_key_changed_after_goal_mutation=true
mutated_workflow_revision=1
```

O comportamento esperado foi confirmado:

- chamadas repetidas com o mesmo workflow/task/budget reutilizam o mesmo cache key;
- uma mutação de goal invalida o fingerprint;
- o fingerprint permanece ligado ao `context_sha256` e ao lineage do pacote.

## Nota sobre o relatório interno

O relatório interno registra bloqueios de instalação global e push observados de dentro do sandbox do Codex. Fora desse sandbox, o wrapper do Forge concluiu instalação, commit e push.

Estado verdadeiro verificado pelo supervisor:

```txt
forge --version = forge 0.4.32
git log -1 = a804211 (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Expor o `routing_fingerprint.cache_key` diretamente em `forge task handoff` e nas projeções de `forge inspect`, para que operadores e executores bounded vejam a chave de cache sem abrir o pacote completo de contexto.

O goal de Personality/Soul Routing continua ativo no prompt de autoevolução; o próximo avanço útil nessa área é projetar persona/soul no handoff e nos artifacts humanos, não apenas no contexto.
