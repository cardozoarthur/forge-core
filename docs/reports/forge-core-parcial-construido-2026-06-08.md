# Parcial do Forge Core - 2026-06-08

Data local do corte: 2026-06-08 21:42 -03.
Repositório: `/home/arthur/projects/forge-core`.
Versão local verificada: `forge 0.4.177`.

## Resumo Executivo

O Forge Core hoje já está bem além de um planejador de tarefas. O corte atual materializa a direção de "workflow operating system": Core Rust, Addons como unidade de capacidade, identidade/multi-tenant, eventos globais, memória governada, dispatch de contratos runtime e superfície MCP/CLI para operação por agentes.

O estado ainda não está pronto para promoção limpa porque o `cargo clippy --all-targets --all-features -- -D warnings` falha em dívida estrutural de lint, principalmente funções grandes com argumentos demais e tipos complexos em `src/addon.rs` e `src/storage.rs`. Compilação, testes e build release passaram.

## Base Observada

- Não há commit novo desde `2026-06-08 00:00:00`; esta parcial reflete o worktree não commitado.
- Alterações rastreadas: 26 arquivos, 14.027 inserções e 1.307 remoções.
- Arquivos novos principais ainda não rastreados:
  - `src/addon.rs` com 7.679 linhas.
  - `src/identity.rs` com 1.800 linhas.
  - `src/event.rs` com 1.589 linhas.
  - `src/cost.rs` com 449 linhas.
  - `tests/forge_addon_architecture.rs` com 7.345 linhas.
  - `docs/forge-operating-system-gap-plan-2026-06-08.md`.
  - `docs/reports/forge-operating-system-cycle-2026-06-08.md`.

## O Que Está Construído

### Core + Addons

- Addon kernel com catálogo, resolução de capacidades, validação, lifecycle persistente, permissões, índice materializado de capacidades e comandos/MCP para instalar, habilitar, desabilitar, atualizar, rebaixar e remover Addons.
- Packaging determinístico de Addons com manifesto canônico, metadados de distribuição, package lock, marketplace local, trust store e verificação Ed25519.
- Compatibility gates por versão do Forge, API, runtime, features, plataforma e migrações.
- Workflow persistente de migração/rollback para mudanças major.
- Runtime contracts declarativos para planners, replanners, validators, executors e handoffs.
- Ledger de dispatch runtime com `queued`, `blocked`, `dry_run`, execução builtin segura, worker registry, claim/completion e assinatura Ed25519 para workers externos assinados.
- Primeiro executor real de worker `local_process`, com comando absoluto, allowlist, JSON stdin/stdout, timeout e completion auditável.
- Views de Addon declarativas para composição de UI/TUI/ops console.

### Identidade, Tenant E Política

- Operating context por projeto com organização, marca, produto, usuário, canal, identidade de marca, design system, política operacional e modo de tenant policy.
- Identity registry persistido em SQLite.
- Memberships com grants, denies, validade temporal e precedência de deny.
- Links cross-channel auditáveis para resolver identidades equivalentes entre Telegram, Discord, Web e outros canais.
- Tenant index físico para workflows, runs, artifacts e events.
- Tenant audit e tenant policy em modos `audit` e `enforce`.
- Enforcement opt-in aplicado a planejamento, eventos, contexto, handoff, leases, requests async, mutações de workflow, checkpoints, interações humanas, schedules, patches e propostas operacionais.

### Eventos E Runtime Operacional

- Event inbox global antes de existir workflow.
- Rotas para iniciar, continuar, modificar, pausar, retomar e completar workflows.
- Completion de workflow segue validação antes de promoção.
- Event adapters declarativos por Addon com transporte, direção, origem, actions, event types, schema, auth e permission gate.
- `events scan` e `events worker` como processamento bounded do inbox.
- Timeline global tenant-aware sobre `global_events`.
- Workflow runtime root persistido com distinção entre workflow efêmero/persistente, lifetime, scale-to-zero e projeção operacional em `forge list` e `forge ops snapshot`.

### Contexto, Memória, Personalidade E Custo

- `forge context` inclui operating context versionado, lineage por hash e propaga brand voice/tone/values, design tokens e política operacional para persona/prompt packet.
- Política de memória com níveis de escopo, filtro tenant e governança de busca.
- `memory search/promote/promotions/retention/cleanup` com promoção curada, aprovação, lineage, retenção e cleanup gated.
- `cost ledger` agrega custo estimado de tasks e custo/tokens observados por workflow, node, tenant e origem de Addon.

### MCP E CLI

- As superfícies novas foram expostas também via MCP para Addons, identidade, eventos, memória, custos e workers.
- `forge plan` carrega Addons do projeto por padrão e grava intent `forge.intent.v2` com capability resolution.
- `forge skill install` mantém a regra correta: detecta Codex/OpenCode/Docker/Kubernetes, mas não autoriza uso automático.

## Evidência Rodada Agora

- `cargo fmt --check`: passou.
- `cargo test`: passou.
  - 88 unitários.
  - 29 testes de arquitetura Addon/identity/event.
  - 290 testes de contrato CLI/MCP.
  - 0 doctests.
- `cargo build --release`: passou.
- `target/release/forge --store /tmp/forge-plan-smoke.sqlite plan --goal 'Create a delivery platform' --output json`: passou, status `planned`, workflow `wf_242cc5a659974f8480283c91ff79f8ff`.
- `target/release/forge --store /tmp/forge-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passou, instalando skills em `/tmp/forge-skill-smoke` e mantendo executores/runtimes como `not_authorized` até aprovação humana.

## Pendências Encontradas

- `cargo clippy --all-targets --all-features -- -D warnings`: falhou.
- Principais categorias:
  - `clippy::too_many_arguments` em funções grandes de `src/addon.rs`, `src/storage.rs` e `src/event.rs`.
  - `clippy::type_complexity` em `src/addon.rs` e `src/storage.rs`.
  - `clippy::needless_borrow` em `src/event.rs`.
  - `clippy::unnecessary_map_or` em `src/improve.rs`.
- Isso não impede build/teste, mas impede chamar o corte de pronto pelos gates do repositório.

## Distância Para A Versão 0.5

O corte aproxima o Forge da 0.5 porque troca heurísticas hard-coded por contratos operacionais: Addons, capabilities, event adapters, runtime contracts, identidade, tenant policy, memória e custo viram superfícies explícitas e auditáveis. A direção está alinhada com Forge como autoridade de orquestração, não com CLI/model provider como fonte de verdade.

Ainda falta para a 0.5:

- limpar o clippy sem enfraquecer o design;
- completar auditoria requisito por requisito contra `goal1.md`, `goal2.md` e `goal3.md`;
- transformar policies de tenant/Addons em UX operacional administrável;
- executar workers remotos WASM/API e adapters externos reais;
- completar renderers seguros para views de Addon além dos cards HTML atuais;
- promover migração/rollback de Addon para execução real com backup/restore efetivo;
- fechar validação final com os gates exigidos antes de commit/promoção.

## Próximo Passo Recomendado

Antes de qualquer commit ou promoção, eu atacaria primeiro o clippy com pequenas estruturas de parâmetros e type aliases nos pontos apontados. Depois disso, rodaria novamente:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
target/release/forge --store /tmp/forge-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json
target/release/forge --store /tmp/forge-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke
```
