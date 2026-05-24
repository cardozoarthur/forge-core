# Forge Core v0.4.29 - Validação de Supervisor

## Estado real no host

- Versão instalada no shell do usuário: `forge 0.4.29`.
- Commit promovido: `292d08d`.
- Estado do GitHub: `HEAD -> main, origin/main`.
- O ciclo `run_4cd2c370782b47ad9162297dc908c39d` terminou com `validation_passed=true`, `self_update.status=completed`, `committed=true` e `public_project_update.status=completed`.

## O que a versão implementou

O Forge agora possui `forge task handoff`, um contrato explícito para entregar uma tarefa a um executor como Codex ou OpenCode.

O handoff combina em um único envelope:

- contexto estrito e pronto para execução;
- `context_sha256` para rastrear exatamente o pacote entregue;
- lease da tarefa;
- executor selecionado;
- tipo de executor esperado pela tarefa;
- saída esperada;
- gate de validação;
- regras de validação;
- política de execução;
- modo de persona.

Isso reduz acoplamento informal entre `forge context --strict` e aquisição de lease, porque o runtime passa a controlar o pacote que autoriza o executor a trabalhar.

## Validação externa executada

Foi feito um smoke test fora do sandbox do executor:

```txt
forge plan --goal "Handoff"
forge task handoff --executor codex
forge task handoff --executor opencode
```

Resultado:

```txt
handoff_ready    true    forge.executor_handoff.v1    lease_acquired    forge.context.v13    sha256=64 bytes
lease_conflict   exit=1  current_lease.executor=codex
```

O comportamento esperado foi confirmado:

- o primeiro executor recebe um handoff autorizado;
- o pacote usa o schema `forge.executor_handoff.v1`;
- o contexto vem de `forge.context.v13`;
- o lease é adquirido;
- uma segunda tentativa concorrente falha com `lease_conflict`.

## Nota sobre o relatório interno

O relatório gerado pelo executor menciona bloqueios de instalação global e push porque ele observou a execução de dentro do sandbox do Codex. O wrapper externo do Forge concluiu a instalação local, o commit e o push depois que o executor terminou.

Para acompanhamento operacional, o estado verdadeiro é o do supervisor:

```txt
forge --version = forge 0.4.29
git log -1 = 292d08d (HEAD -> main, origin/main)
```

## Próximo ciclo recomendado

Adicionar modo de retomada em `forge task handoff`, exigindo match de checkpoint/lineage antes de emitir novo lease para retry parcial ou continuação assíncrona.

O goal novo de Personality/Soul Routing já está no prompt de autoevolução ativo e deve ser tratado como evolução estrutural: persona por node, lineage auditável, contexto mínimo e validação de artefatos humanos.
