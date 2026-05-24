# Forge Core v0.4.31 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.31`.
- Commit promovido pelo workflow: `f1c035c`.
- Estado do GitHub após promoção: `HEAD -> main, origin/main`.
- O ciclo `run_fa7a315203684ab0a4a58b692cc743a0` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora expõe a rota de contexto por node em `forge inspect --output json`.

Cada `nodes[].context_route` inclui:

- `schema_version`;
- política de roteamento;
- perfil do executor;
- flags de reasoning/determinismo;
- orçamento solicitado e efetivo;
- bytes selecionados;
- `context_sha256`;
- readiness do contexto;
- readiness/status de handoff;
- status de resume/checkpoint;
- seções obrigatórias ausentes;
- seções incluídas/omitidas;
- resumo de shards.

O diagrama terminal também passa a anotar cada task com `context <profile> <handoff_status> <selected>/<effective>`.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com"
forge inspect <workflow_id> --output json
```

Resultado para a task `Run deterministic non-AI step`:

```txt
schema=forge.context.v14
profile=no_ai_deterministic
reasoning_allowed=false
deterministic=true
handoff_status=blocked_missing_context_and_dependencies
resume_context_status=no_checkpoint
context_sha256_length=64
diagram_context_route=ok
```

Detalhe do bloqueio real:

```json
{
  "missing_required_sections": ["context_requirements"],
  "included_sections": ["local_objective", "execution_policy", "validation_rules"],
  "omitted_sections": ["context_requirements", "workflow_goal", "dependencies", "work_item", "constraints"],
  "context_ready": false,
  "handoff_ready": false
}
```

O comportamento esperado foi confirmado: o operador consegue enxergar, em um único `inspect`, qual perfil de executor será usado, se a task é determinística, qual checksum de contexto foi calculado e por que ela ainda não pode ser entregue via handoff.

## Nota sobre o relatório interno

O relatório interno registra bloqueios de instalação global e push observados de dentro do sandbox do Codex. Fora desse sandbox, o wrapper do Forge concluiu instalação, commit e push.

Estado verdadeiro verificado pelo supervisor:

```txt
forge --version = forge 0.4.31
git log -1 = f1c035c (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Adicionar fatias explícitas de checkpoint freshness e partial retry no `forge list` e no `forge inspect`, para separar tasks prontas, stale-resume, bloqueadas por dependência e bloqueadas por contexto antes de `forge task handoff`.

O goal de Personality/Soul Routing continua ativo. A capacidade já existe como persona por node, source models Codex/Paperclip, hash em lineage, contexto roteado e validação de promoção; o próximo avanço útil é conectá-la ao handoff ou criar modos explícitos de persona para artifacts humanos.
