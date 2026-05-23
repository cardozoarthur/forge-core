# Forge Core v0.4.7 Goals Report - 2026-05-23

## Objetivo

Adicionar ao Forge objetivos persistentes para evoluir de executor de DAG local para runtime observável, componível e capaz de operar flows finitos e infinitos.

## Goals adicionados

- `forge list` deve listar workflows/runs rodando e não rodando.
- A listagem deve expor id estável, estado de lifecycle e descrição do pedido inicial do flow.
- Workflows finitos devem escalar para zero quando não houver trabalho executável ou agendado.
- Workflows/subflows infinitos devem ficar vivos como entidades scheduláveis, mesmo quando estiverem ociosos.
- `forge inspect <id>` deve desenhar o grafo do workflow no terminal.
- `forge inspect <id> --verbose` deve incluir subflows e descrição de cada processo/subprocesso.
- Workflows devem aceitar subflows recursivos: flow com vários subflows, e cada subflow com seus próprios subflows.
- Antes de criar um novo workflow, Forge deve procurar flows disponíveis e integrar os compatíveis como filhos quando isso reduzir duplicação.

## Impacto

Esses goals passam a aparecer na definição técnica, no roadmap e no prompt de autoevolução. Os próximos ciclos do Forge devem priorizar incrementos pequenos e validados nessa direção, começando por contratos de CLI e persistência.

## Próximo passo recomendado

Implementar primeiro o contrato de `forge list` com lifecycle básico e pedido inicial preservado. Depois implementar `forge inspect` simples, antes de expandir para subflows infinitos e composição automática.
