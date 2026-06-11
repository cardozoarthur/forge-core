# Forge Operating System Gap Analysis And Incremental Plan

Data: 2026-06-08

## Fonte Da Verdade

Este plano trata como especificação oficial os documentos:

- `/home/arthur/Downloads/goal1.md`
- `/home/arthur/Downloads/goal2.md`
- `/home/arthur/Downloads/goal3.md`

Objetivo consolidado: transformar o Forge em uma infraestrutura operacional AI-native, workflow-first, event-driven, domain-agnostic, multi-tenant e human-in-the-loop, com Core mínimo e Addons de primeira classe. ForgeFlow deve ser tratado como o primeiro alfa histórico do Forge, útil como fonte de aprendizado, mas não como destino arquitetural final.

## Estado Atual Observado

### Forge Core

- Repositório: `/home/arthur/projects/forge-core`
- Stack: Rust CLI + biblioteca, SQLite local, MCP surface e skill local.
- Versão local antes deste corte: `forge 0.4.177`.
- Pontos fortes já existentes:
  - workflow store, event store, runs, leases e checkpoints;
  - DAG de tarefas atômicas com validação e rework;
  - waits, schedules, resume e partial retry;
  - context routing avançado com lineage e persona contract;
  - brain routing por node com hot-swap de Codex/OpenCode/Gemini/Claude;
  - human interactions, approvals e decisões de produto;
  - creative artifacts, tokens, whiteboard/collaboration events e ops console local;
  - runtime guard, cluster placement dry-run, cost accounting e improvement candidates.

### ForgeFlow Alfa

- Repositório: `/home/arthur/projects/forge-flow`.
- Estado: alfa operacional em Python/FastAPI/Kubernetes/Postgres, sem git no diretório atual.
- Conceitos reaproveitáveis:
  - ingressos reais por Telegram/webhook;
  - runtime web/API com Postgres;
  - deploy Kubernetes/Traefik;
  - padrão de relatório operacional e integração com canais externos.
- Limite: não deve virar o Core final. O que for reaproveitado precisa entrar como contrato de Core ou Addon, não como acoplamento Python específico.

## Benchmark Inicial

Fontes consultadas em 2026-06-08:

- Gemini CLI: https://github.com/google-gemini/gemini-cli
- Codex CLI: https://github.com/openai/codex
- Claude Code CLI: https://code.claude.com/docs/en/cli-usage
- OpenCode: https://github.com/opencode-ai/opencode
- OpenClaw: https://openclaw.ai/ e https://github.com/openclaw/openclaw
- Hermes Agent: https://github.com/nousresearch/hermes-agent
- OpenSquad: https://openmoss.cn/en/
- Open Design: https://github.com/nexu-io/open-design
- Penpot: https://github.com/penpot/penpot
- Paperclip: https://github.com/paperclipai/paperclip
- Remotion: https://github.com/remotion-dev/remotion
- n8n: https://github.com/n8n-io/n8n e https://docs.n8n.io/integrations/builtin/node-types/

Ideias a absorver sem copiar arquitetura:

| Projeto | Melhor ideia para o Forge | Decisão arquitetural |
| --- | --- | --- |
| Gemini CLI | TUI terminal-first, MCP, non-interactive JSON/stream JSON, checkpointing e contexto de projeto. | Forge deve ter TUI forte, eventos streamáveis e executor adapters intercambiáveis. |
| Codex CLI | Agente local pragmático com sandbox, comandos e integração de edição/testes. | Codex é brain de execução, não autoridade do workflow. Forge deve manter estado, contexto e validação. |
| Claude Code CLI | CLI de longa duração, comandos, MCP e operação em projetos reais. | Reaproveitar a ideia de dynamic workflows e permissões, mantendo Forge como event/workflow OS. |
| OpenCode | UX terminal eficiente e provider/model abstraction. | Forge deve ser agnóstico ao brain e permitir múltiplos brains por workflow/node. |
| OpenClaw | Assistente operacional multi-canal, memória persistente, skills e operação 24/7. | Canais e skills devem virar Addons/event adapters; estado e permissões ficam no Forge. |
| Hermes Agent | Learning loop, skills que evoluem, memória em arquivo e execução remota. | Forge deve separar memória global/organização/projeto/processamento e promover skills por evidência. |
| OpenSquad | Collab Cards, boards por agente, plugin architecture, VCS audit, multi-channel e dynamic MCP. | Forge deve modelar colaboração como eventos e expor task boards/agents como views de workflow. |
| Open Design | Design local-first, skills, exports e bridge com vários CLIs. | Design deve ser Addon visual, com IR Forge-owned antes de exportar para ferramentas. |
| Penpot | Design tokens, componentes, API/plugin system, self-hosting e colaboração. | Tokens/componentes/views devem ser artefatos genéricos e composáveis por Addons. |
| Paperclip | Governança de organização, orçamentos, auditoria e gestão de agentes como empresa. | Multi-tenant/org model deve ser nativo; budgets e audit logs devem ser universais. |
| Remotion | Vídeo programático com React e pipeline renderizável. | Conteúdo e vídeo entram como Addons de artifact/render pipeline, não no Core. |
| n8n | Trigger/action node library, credenciais, visual workflow e marketplace de nodes. | Forge deve absorver trigger/action/capability registry, mas com estado, validação e contexto próprios. |

## Gaps Principais

1. Addons ainda não eram conceito de primeira classe.
   - Antes deste corte, capacidades específicas apareciam como heurísticas no Core.
   - Exemplo: `intent.rs` e `graph.rs` continham gatilhos explícitos para n8n, hackathon, visual workspace, Telegram e daily research.

2. Multi-tenancy ainda era implícita.
   - Havia memória com noções de privacidade/escopo, mas o workflow não carregava organização, marca, produto, usuário e canal como contexto obrigatório.

3. Event-driven ainda é workflow-scoped.
   - Existe tabela `events`, mas eventos são ligados a workflow existente. Falta event bus genérico capaz de iniciar, continuar, pausar, retomar, modificar e encerrar workflows de qualquer origem.

4. Persistent workflows existem parcialmente.
   - Schedules, loops e waits existem, mas falta distinguir claramente `ephemeral_workflow` e `persistent_workflow` no contrato de intent/workflow.

5. Dynamic Workflow Engine ainda é parcialmente template-driven.
   - O planner base ainda monta tarefas por caminhos definidos no Core.
   - A direção correta é capability-driven planning com hooks de Addons.

6. UI/TUI ainda não é classe mundial.
   - Há REPL, slash commands, inspect e ops console, mas falta TUI completa com DAG em tempo real, painéis de workflow/handoff/checkpoint/wait, custos, contexto, timeline e composição por Addons.

7. Addon lifecycle não está completo.
   - Falta install/uninstall/enable/disable/upgrade/downgrade persistente, compat validation, dependency resolver, permissões granulares e marketplace packaging.

8. Segurança de Addons ainda é declarativa.
   - O manifesto precisa virar gate real de execução: ferramentas, recursos, integrações e permissões devem bloquear uso até autorização.

9. ForgeFlow alfa precisa ser absorvido seletivamente.
   - Telegram/webhook/Kubernetes/Postgres devem virar runtime/event/notification Addons ou adapters.
   - O Python/FastAPI alfa não deve contaminar o Core Rust como dependência estrutural.

## Conflitos E Decisões

- Conflito: Core atual tem valor real, mas contém domínio.
  Decisão: preservar contratos de execução e mover gatilhos para capability registry, mantendo first-party Addons de compatibilidade enquanto os Addons externos amadurecem.

- Conflito: ForgeFlow funciona como alfa, mas é outro runtime.
  Decisão: usar ForgeFlow como fonte de requisitos operacionais e migrar capacidades para Forge Core + Addons.

- Conflito: muitos benchmarks são ferramentas de software.
  Decisão: absorver TUI, eventos, permissões, memória, execução e observabilidade, sem tornar o Forge especializado em desenvolvimento.

- Conflito: Addons compilados no Core ainda não são desacoplamento perfeito.
  Decisão: aceitar first-party builtin compatibility Addons como etapa de transição, desde que o Core também carregue manifestos externos em `.forge/addons`.

## Oportunidades De Reutilização

- `context.rs`: já é um bom kernel de Context Routing; deve receber contexto organizacional e capability lineage.
- `graph.rs`: já tem tasks, waits, checkpoints, handoffs, brain routing e persona. Deve consumir Addon workflow extensions em vez de heurísticas.
- `storage.rs`: já persiste workflows/events/runs; deve ganhar tabelas globais de organizations, identities, addons, capabilities e event inbox.
- `ops.rs`: já tem console local e visual artifacts; deve virar UI composition host.
- `mcp.rs`: já expõe ferramentas para agentes; deve expor addon registry/capability discovery.
- `memory.rs`: já modela escopos; deve integrar organization/project/processing memory governance.
- `forge-flow`: reaproveitar Telegram ingress, Postgres runtime, Kubernetes deployment e relatório operacional como adapters.

## Primeiro Corte Implementado

Este corte iniciou a migração para Core + Addons:

- Adicionado `src/addon.rs`.
- Adicionado `forge addons catalog`.
- Adicionado `forge addons resolve --goal ...`.
- Adicionado carregamento de manifestos externos em diretórios `--addon-dir`, com default `.forge/addons`.
- Expostos `forge.addons.catalog` e `forge.addons.resolve` no MCP.
- `forge plan` agora carrega Addons do projeto em `.forge/addons` por padrão.
- Adicionado `forge addons validate`, com validação de ids duplicados, dependências de Addon, dependências de capacidade e permissões de alto risco.
- `forge addons validate` agora valida `version_req` de dependências de Addon com operadores comuns (`>=`, `<=`, `>`, `<`, `=`, `^`, `~`).
- Exposto `forge.addons.validate` no MCP.
- Adicionada tabela `installed_addons` no SQLite.
- Adicionada tabela `addon_capabilities` no SQLite como índice materializado das capacidades instaladas.
- Adicionados `forge addons installed|install|upgrade|downgrade|enable|disable|uninstall`.
- Adicionado `forge addons package`, emitindo `forge.addon_package.v1` com hash de manifesto, metadados de distribuição, resumo de capacidades/dependências/permissões/contratos/views e assinatura destacada declarativa para preparação de marketplace.
- Adicionado `forge addons capabilities`, com filtros por Addon, capacidade e lifecycle.
- Adicionada tabela `addon_permission_authorizations` no SQLite.
- Adicionados `forge addons permissions|authorize-permission|revoke-permission`, com approval/revocation auditável.
- Install/enable de Addons instalados agora bloqueiam permissões declaradas com `requires_human_approval` sem autorização persistente.
- Revogar autorização muda capacidades instaladas para lifecycle `unauthorized`, removendo-as da resolução ativa sem apagar o histórico.
- Expostos `forge.addons.installed|install|upgrade|downgrade|enable|disable|uninstall` no MCP.
- Exposto `forge.addons.package` no MCP como contrato de package local determinístico sem instalar, buscar ou executar Addons.
- Exposto `forge.addons.capabilities` no MCP.
- Expostos `forge.addons.permissions|authorize_permission|revoke_permission` no MCP.
- Catálogo combinado agora mescla built-ins, `.forge/addons` e Addons instalados; o registro instalado controla lifecycle para o mesmo id.
- Install/upgrade/downgrade/enable/disable/uninstall atualizam o índice materializado, permitindo consulta de capacidades instaladas sem reprocessar manifestos inteiros.
- `forge addons upgrade|downgrade --manifest ...` preservam o lifecycle existente, exigem que a versão candidata suba/desça na direção correta, revalidam o catálogo e rematerializam capabilities com a versão atual do manifesto.
- Adicionado `CapabilityResolutionReport` com:
  - capacidades requeridas;
  - capacidades ausentes;
  - addons ativos;
  - ativações de workflow extension com lineage de Addon/capacidade;
  - overlay de constraints/deliverables/risks/unknowns;
  - workflow extensions.
- Evoluído `IntentSpec` para `forge.intent.v2`, mantendo compatibilidade de deserialização com defaults:
  - `workflow_mode`;
  - `event_policy`;
  - `operating_context`;
  - `required_capabilities`;
  - `active_addons`;
  - `capability_resolution`.
- O planner em `graph.rs` agora aciona extensões principais via capacidades resolvidas para:
  - workflow automation/n8n research;
  - hackathon factory;
  - daily goal research;
  - async runtime.
- O planner passou a checar extension ids declarados antes de capability ids legados para os caminhos first-party conhecidos.
- Os builders first-party de workflow extension agora passam por um registry interno com extension id, capability id, fase, guard textual legado e função builder, reduzindo ifs espalhados em `build_tasks`.
- O registry first-party agora prefere ativações vindas de `CapabilityResolutionReport`; guards textuais só acionam intents legados sem evidência de resolução de capacidades.
- Workflow extensions externas declaradas em manifesto agora geram tarefas genéricas auditáveis no DAG quando não há builder first-party, preservando lineage de Addon, versão, capability e extension.
- Adicionado `src/identity.rs` e `forge identity context --project-root ...`, carregando `.forge/operating-context.yaml|yml|json`.
- `OperatingContextSpec` agora inclui `brand_identity`, `design_system` e `operating_policy`, além de organização/marca/produto/usuário/canal/memória/personality.
- Adicionada tabela `identity_registry` no SQLite.
- Adicionados `forge identity sync` e `forge identity registry`, materializando organização, marca, produto, usuário e canal do operating context.
- Adicionada tabela `identity_memberships` no SQLite.
- `forge identity sync` agora também materializa membership ativo do usuário do operating context no escopo organização/marca/produto.
- Adicionado `forge identity memberships`, com filtros por subject, organização, marca, produto e status.
- Adicionada tabela `identity_links` no SQLite para equivalência auditável cross-channel.
- Adicionados `forge identity link|unlink|links|resolve`, permitindo vincular/desvincular Telegram, Discord, Web ou outros ids a uma identidade governada sem apagar histórico.
- Expostos `forge.identity.link|unlink|links|resolve` no MCP.
- `tenant-policy` agora resolve links ativos antes de checar memberships, permitindo que uma identidade de canal autorize por uma membership da identidade governada vinculada.
- Adicionada tabela `tenant_index` no SQLite para workflows, runs, artifacts e events.
- Adicionado `forge identity tenant-index`, com filtros por resource type, organização, marca, produto e workflow.
- Adicionado `forge identity tenant-audit`, comparando workflows, runs, artifacts e events persistidos contra `tenant_index`.
- Adicionado `forge identity tenant-policy`, com modos `audit` e `enforce`, validando contexto organizacional explícito, membership ativo e cobertura do tenant index.
- `forge.identity_memberships.v1` agora expõe permissões derivadas do papel e ambientes do membership.
- `forge.identity_sync.v1` materializa membership `operator` com permissões operacionais iniciais.
- Identity registry, membership data e event stream projetam identidade de marca, design system e políticas do operating context.
- `forge.tenant_policy.v1` agora inclui `action`, `required_permission`, `membership_roles`, `granted_permissions` e gate `membership_permission`.
- `OperatingContextSpec` agora inclui `tenant_policy_mode`, com default `audit`.
- Quando `tenant_policy_mode: enforce`, `forge plan` e `forge events route` para `start_workflow` bloqueiam criação de workflow sem contexto explícito e membership ativo.
- `forge request start` agora também carrega operating context e Addons do projeto, alinhando async requests ao mesmo contrato de `forge plan`.
- Adicionado `ensure_workflow_policy` para workflows existentes, bloqueando em modo `enforce` quando membership ou cobertura do `tenant_index` faltam.
- Enforcement opt-in agora cobre context request, task handoff/leases, request drive/step/status/heartbeat/final-package/switch/cancel/resume/recover, final audit, workflow mutations, checkpoints, human interactions, schedule update/run-due, patch plan/apply/revert e ops modifier proposals.
- Adicionado `src/event.rs` e `forge events list --workflow ...`, projetando o event log em envelopes tenant-aware.
- Adicionada tabela global `event_inbox` com `forge events ingest|inbox|route`.
- Primeiros roteamentos universais implementados:
  - `start_workflow` cria workflow a partir de evento antes de existir workflow;
  - `continue_workflow` continua workflow/run existente por payload genérico, com suporte inicial para anexar artefato, registrar checkpoint, responder interação humana, completar task pronta e dirigir run assíncrono;
  - `modify_workflow`/`update_goal` altera objetivo via mutação revisionada;
  - `pause_workflow` e `resume_workflow` alteram estado via revisão/evento;
  - `complete_workflow` só conclui se `validate_workflow` estiver promotable.
- Expostos `forge.identity.context` e `forge.events.list` no MCP.
- Expostos `forge.identity.sync` e `forge.identity.registry` no MCP.
- Exposto `forge.identity.memberships` no MCP.
- Exposto `forge identity membership-update` e MCP `forge.identity.membership_update` para role/status, grants, denies e validade temporal sem editar `data_json`.
- Exposto `forge.identity.tenant_index` no MCP.
- Exposto `forge.identity.tenant_audit` no MCP.
- Exposto `forge.identity.tenant_policy` no MCP.
- Expostos `forge.events.ingest`, `forge.events.inbox`, `forge.events.route`, `forge.events.scan` e `forge.events.worker` no MCP.
- Manifestos de Addon agora declaram `event_adapters` com transporte, direção, origens, actions, event types, schema e auth.
- Exposto `forge events adapters` e MCP `forge.events.adapters` como descoberta de contratos de ingress/egress por Addon.
- `forge events route` agora avalia `forge.event_adapter_policy.v1` antes da rota, usando adapters declarados para validar origem/transporte, action, schema e permission gate; eventos sem adapter declarado continuam compatíveis como `no_declared_adapter`.
- Manifestos de Addon agora declaram `runtime_contracts` para `planning_strategy`, `replanning_strategy`, `validator`, `executor` e `handoff`.
- Exposto `forge addons contracts` e MCP `forge.addons.contracts` como descoberta de contratos runtime por Addon/capability/lifecycle.
- Exposto `forge addons contract-policy` e MCP `forge.addons.contract_policy` como preflight de dispatch seguro para contratos runtime, validando lifecycle, permission gate, runtime e entrypoint.
- Exposto `forge addons dispatch-contract|dispatch-planner|dispatches` e MCP `forge.addons.dispatch_contract|dispatch_planner|dispatches` como ledger persistente de dispatch runtime, registrando envelopes `queued`, `blocked` e `dry_run` sem executar código externo inline. `dispatch-planner` restringe o contrato a `planning_strategy`/`replanning_strategy` e padroniza o input como `forge.addon_planner_dispatch_input.v1`.
- Exposto `forge addons run-dispatch` e MCP `forge.addons.run_dispatch` como processador seguro do ledger: ele reavalia a policy atual, bloqueia revogação/drift pós-enqueue, conclui apenas `forge_core_builtin` allow-listed e marca runtimes externos como `needs_external_worker`.
- Exposto `forge addons register-worker|workers` e MCP `forge.addons.register_worker|workers` como registry auditável de workers externos por runtime/status/trust level; `run-dispatch` agora inclui workers elegíveis na evidência de runtime externo.
- Exposto `forge addons claim-dispatch|complete-dispatch` e MCP `forge.addons.claim_dispatch|complete_dispatch` como protocolo de worker externo: claim exige worker registrado/compatível, recheck de policy e snapshot de identidade/chave; completion valida ownership, exige assinatura Ed25519 válida contra o snapshot do claim para trust `signed|trusted`, grava hash de resultado/attestation e bloqueia revogação pós-claim.
- Exposto `forge addons execute-dispatch` e MCP `forge.addons.execute_dispatch` como primeiro executor real de worker: roda workers registrados com `execution_mode: local_process`, comando absoluto, allowlist de entrypoint/contract, JSON stdin/stdout, timeout e completion pelo mesmo ledger/signature gate; também roda workers `execution_mode: external_api` por endpoint HTTP/HTTPS explícito, hosts locais por default ou hosts remotos allowlisted, request/response JSON tipado, timeout, limite de resposta e auth Bearer/HMAC por env ou credential-vault.
- Exposto `forge addons execute-planner` e MCP `forge.addons.execute_planner` como executor auditável para `planning_strategy`/`replanning_strategy`: cria plano Core de referência, injeta `context.core_reference`, executa worker registrado, valida shape de tasks, compara ids/títulos/dependências/regras de validação e só marca `planning_strategy_equivalence_validated` quando o planner externo está pronto para substituição.
- Exposto `forge harness token-headroom|retrieve-headroom|wrap-plan|install-shims|shim-status|exec|mode` e MCP `forge.harness.token_headroom|retrieve_headroom|wrap_plan|install_shims|shim_status|exec|mode`, inspirado no Headroom, para reduzir contexto localmente por tipo de conteúdo, persistir blobs reversíveis de headroom em SQLite, recuperar payloads por retrieval ref, gerar planos de wrapper CLI Forge-first para Codex/Claude/Gemini/OpenCode, instalar shims PATH não destrutivos quando o usuário preferir a infraestrutura do Forge, auditar precedência/ownership/recursão dos shims, emitir recibos dry-run/guarded de execução e resolver a política efetiva Forge-first sem executar CLIs. O instalador de shims agora resolve o CLI nativo automaticamente via `PATH` quando `real_cmd` não é informado, exclui a própria pasta de shims para evitar recursão e registra origem/status de resolução. `shim-status` lê o script Forge-owned, reporta real CLI/store/Forge binary, detecta shims manuais que tomariam precedência e devolve instruções sem executar processos. `forge harness mode` aplica precedência `observe_only_flag > explicit_flag > env_default > project_config > default_observe_only`, lê `.forge/harness.json` quando existir e expõe status seguro para TUI/MCP antes de `wrap-plan`, `install-shims` ou `exec`. `forge sync executors --shim-dir` agora projeta esse diagnóstico como `forge.executor_harness_status.v1`, `forge_first_ready` e entrypoints Forge-first no relatório de executores, no brain router e nas sessões `/brains`/`/shells`. Execuções reais autorizadas agora aplicam a mesma política de headroom ao stdout/stderr do processo filho, persistem refs reversíveis por stream e mantêm hashes/excertos no recibo `forge.harness.exec_receipt.v1`.
- Exposto `forge interactive readiness` e MCP `forge.interactive.readiness` como painel dedicado `forge.interactive.readiness.v1` para readiness de executores, runtimes, brains, shells, superfícies controladas pelo Forge, harness mode, harness doctor e próximos comandos corretivos antes de abrir shell ou fazer handoff, sem carregar todo o `interactive home`.
- Exposto `forge interactive harness` e MCP `forge.interactive.harness` como painel dedicado `forge.interactive.harness.v1` para agregar harness mode, doctor, shim status, wrap-plan e prévia de token-headroom de um brain CLI; o painel também entra em `dashboard.harness_panel` e no `ui_composition_panel` como widget Core para TUI/web/agentes sem instalar shims nem executar CLIs.
- Exposto `forge interactive patch-workbench` e MCP `forge.interactive.patch_workbench` como painel dedicado `forge.interactive.patch_workbench.v1` para status Git, contagens de arquivos staged/unstaged/untracked, diff stat/check, lanes por arquivo e comandos de lifecycle `patch plan/review/diff/apply/restore`; o painel também entra no `interactive home` e no `ui_composition_panel` como widget Core para TUI/web sem mutar arquivos.
- Exposto `forge interactive permissions` e MCP `forge.interactive.permissions` como painel dedicado `forge.interactive.permissions.v1` para memberships multi-tenant, autorizações de permissões de Addons, interações humanas pendentes/expiradas e comandos granulares de membership/addon/approval; o painel também entra no `interactive home` e no `ui_composition_panel` como widget Core.
- `.forge/harness.json` agora aceita `require_lineage_for_exec: true`; nesse modo, `forge harness mode --project-root <project-root>`/MCP `forge.harness.mode` com `project_root` expõem `project_exec_policy_status`, `require_lineage_for_exec` e safety checks antes da execução, enquanto `forge harness wrap-plan --project-root <project-root>`/MCP `forge.harness.wrap_plan` com `project_root` fazem o plano de wrapper respeitar os defaults Forge-first do projeto-alvo, e `forge harness install-shims --project-root <project-root>`/MCP `forge.harness.install_shims` com `project_root` instalam shims com os mesmos defaults remotos. `forge harness exec --project-root <project-root>`/MCP `forge.harness.exec` com `project_root` usam a política do projeto-alvo sem mudar o `cwd` do processo filho, bloqueando execução real com `harness_exec_blocked_by_project_policy` até receberem `workflow`, `task` e `run`, mantendo CLIs externos dentro de lineage auditável antes de chamar Codex/OpenCode/Gemini/Claude.
- Exposto `forge addons views` e MCP `forge.addons.views` como descoberta de views/widgets por Addon, surface e lifecycle para UI composition.
- Exposto `forge addons observability` e MCP `forge.addons.observability` como `forge.addon_observability.v1`, consolidando Addons, lifecycles, capabilities, dependências, permissões, gates, runtime contracts, views, artifact/event types, integrações, fluxo ingress/egress de eventos e uso do ledger de dispatch em uma visão operacional.
- Exposto `forge events webhook-ingress` como primeiro listener HTTP real de ingresso: aceita POST JSON em endpoint local bounded, normaliza payload para `event_inbox`, injeta `transport: webhook` e schema opcional, valida HMAC-SHA256 opcional por segredo em variável de ambiente, grava `auth_verified`, bloqueia assinatura ausente/inválida antes do inbox e pode rotear imediatamente pelo mesmo `forge.event_adapter_policy.v1` de Addons ingress.
- Exposto `forge events service-plan` e MCP `forge.events.service_plan` como contrato plan-only de serviço gerenciado para `worker` e `webhook_ingress`, com comando reexecutável, lease TTL, heartbeat, backoff, shutdown cooperativo, health checks e auditoria em `global_events`.
- Adicionada tabela `event_services`, `forge events service-run`, `forge events services`, MCP `forge.events.service_run` e MCP `forge.events.services`; worker e webhook ingress agora podem rodar como serviços gerenciados bounded com lease persistente, health, status final e auditoria em `global_events`. O worker renova heartbeat/lease por ciclo e aceita `--stop-file`/`stop_file` para shutdown cooperativo com status final `stopped`; o webhook ingress persiste heartbeats de progresso enquanto escuta/aguarda requests, aceita o mesmo stop-file entre requests, grava `webhook_report` e health final de requests/ingest/route/stop.
- Exposto `forge events service-supervise` e MCP `forge.events.service_supervise` como primeiro supervisor bounded (`forge.event_service_supervisor.v1`) sobre `service-run`, com reinícios controlados, backoff executável em falha, parada por stop-file, health agregado e auditoria `event_service_supervisor` em `global_events`.
- Exposto `forge events runtime-reconcile` e MCP `forge.events.runtime_reconcile` como primeira ponte entre `forge.registry_workflow_runtime.v1`, inbox e `event_services`: o relatório `forge.event_runtime_reconcile.v1` identifica workflows persistentes que precisam acordar por eventos, eventos pendentes, leases ativos/obsoletos de worker e recomenda ou executa `service-supervise` bounded quando não há worker ativo.
- Exposto `forge events runtime-daemon` e MCP `forge.events.runtime_daemon` como primeiro daemon bounded de reconciliação, persistido em `event_services` como `runtime_reconcile`, com lease/heartbeat próprios, ciclos de `runtime-reconcile`, stop-file cooperativo, health agregado e auditoria `event_runtime_daemon` em `global_events`.
- `runtime-reconcile` e `runtime-daemon` agora aceitam `--scan-schedules`/`scan_schedules` com executor, max-workers e TTL próprios: quando habilitado, cada ciclo inclui `forge.schedule.worker_status.v1`, executa `forge.schedule.scan_due.v1` se `--execute` estiver ativo, agrega `schedule_execution_count`/`schedule_scale_to_zero_count` e reidrata cron ou wait_until due sem iniciar worker de inbox/webhook quando o workflow é schedule-only.
- `forge.registry_workflow_runtime.v1` agora diferencia ações operacionais de schedule (`run_due_schedule`, `sleep_until_schedule`) das ações de evento (`keep_event_listener_ready`, `wake_on_event`), evitando que workflows com cron ou wait_until sejam tratados como necessidade de listener de evento.
- Objetivos com `wait until <RFC3339>` agora geram schedule one-shot `kind=wait_until`; `schedule scan-due` e `events runtime-daemon --scan-schedules` executam o due wait sob lease, registram histórico, limpam `next_run_at` e concluem o task de espera.
- `events runtime-daemon` e MCP `forge.events.runtime_daemon` agora suportam modo contínuo com `--continuous`/`continuous`, saída cooperativa por `idle_exit` ou stop-file, retenção limitada de relatórios por `--cycle-retention`/`cycle_retention` e contadores agregados `retained_cycle_count`/`dropped_cycle_count`.
- `events services-recover` e MCP `forge.events.services_recover` agora marcam serviços `running` com lease vencida como `stale`, preservando o payload de health e adicionando marcador `forge.event_service_recovery_marker.v1` com origem, lease e heartbeat observados.
- `events runtime-reconcile`/`runtime-daemon` e MCP `forge.events.runtime_reconcile`/`runtime_daemon` agora aceitam `--recover-stale-services`/`recover_stale_services`, aplicando a recuperação de leases `worker` vencidas antes de calcular active/stale service counts e recomendações de supervisor.
- Exposto `forge events emit` e MCP `forge.events.emit` como primeiro emissor real de `event_adapters` de egresso: seleciona adapter `egress|bidirectional`, valida direção, origem, action, event type, permission gate e allowlist de host, envia envelope `forge.event_egress_request.v1` para endpoint `http://`/`https://` ou transporte Telegram, grava auditoria `event_egress` em `global_events` com operating context do projeto, e retorna `forge.event_egress_emit.v1` com `global_event_id`, status HTTP e hash da resposta.
- `events emit` non-dry-run agora aplica `ensure_workflow_policy` antes de qualquer transporte externo quando o payload traz `workflow_id` e `ensure_operating_context_policy` quando não há workflow alvo; em `tenant_policy_mode: enforce`, a ação `event egress delivery` exige `workflow:deliver`, bloqueando entrega externa antes de conectar quando membership/tenant policy não permite.
- Adapters de egresso com `auth: hmac` agora podem declarar `secret_env`/`hmac_secret_env` e `signature_header`; o Forge assina o body JSON com HMAC-SHA256, envia `sha256=<hex>` no header e registra somente nome do env/header nos relatórios.
- Adapters de egresso com `auth: bearer` agora podem declarar `secret_env` e opcionalmente `signature_header`; o Forge injeta `Authorization: Bearer ...` por padrão, bloqueia CR/LF em headers e registra somente auth scheme, nome do env/header e nunca o token.
- Adapters de egresso com `auth: hmac`, `auth: bearer` ou Telegram `bot_token` agora também podem declarar `credential_vault` com `vault_bin`, `contract`, `data`, `record` e `field`; o Forge resolve o segredo pelo credential-vault somente no momento da entrega, registra `secret_source=credential_vault` e expõe apenas metadados de contrato/record/field, nunca o valor secreto.
- O Addon first-party de notificações agora declara ingresso `telegram.bot_updates` e egressos governados `telegram.bot_send_message`/`telegram.bot_send_document` com transporte `telegram` e auth `bot_token` para mensagem, documento e relatório. `forge events emit --dry-run` e MCP `forge.events.emit` validam policy, permissão, schema `telegram.report`, env `TELEGRAM_BOT_TOKEN` e auditoria `global_event_id`; non-dry-run usa a Bot API via `curl`, resolve chat por payload/env, tem modo `FORGE_TELEGRAM_EGRESS_MODE=simulate` para validação sem API real e, quando há `payload.workflow_id`, anexa `forge.event_egress_delivery_evidence.v1` ao workflow como `telegram_delivery_record`.
- Exposto `forge events worker` e MCP `forge.events.worker` como loop local bounded (`forge.event_worker_loop.v1`) sobre o inbox, com `max_cycles`, `interval_seconds`, `idle_exit`, `stop_file` cooperativo e agregação de métricas por ciclo.
- Exposto `forge addons planners` e MCP `forge.addons.planners` como `forge.addon_planner_registry.v1`, registrando `planning_strategy`/`replanning_strategy` de Addons e separando builders first-party internos de planners externos por contrato runtime.
- `forge.capability_resolution.v1` agora emite `capability_suggestions` para capacidades ausentes encontradas em Addons conhecidos ou pacotes confiáveis do marketplace local persistido, apontando ação, status lifecycle, pacote, comando CLI e ferramenta MCP para habilitar Addon, autorizar permissão, instalar fonte de catálogo ou instalar package confiável.
- Builders first-party de workflow extension agora são acionados por capability/extension resolvida e mantêm gatilho textual apenas como fallback de compatibilidade para intents antigos.
- Gatilhos de capabilities first-party foram estreitados para não acionar builders especializados por termos genéricos como `hackathon` ou `automation` quando o objetivo exige apenas pesquisa/schedule.
- Views de Addon agora aceitam contrato declarativo com `type`, `component`, `route`, `layout`, `data_bindings`, `actions` e `props`, permitindo dashboards, widgets, visualizações, editores e ferramentas especializadas sem hard-code de domínio no Core.
- Permissões de Addon agora declaram ferramentas, recursos, integrações, actions e escopos tenant; contracts, event adapters e views projetam `forge.addon_permission_gate.v1` com status `allowed`, `missing_human_approval`, `undeclared_permission` ou `addon_not_enabled`.
- `forge addons validate` agora bloqueia manifesto que referencia permissão não declarada em `runtime_contracts`, `event_adapters` ou `views`.
- Catálogo store-aware agora projeta Addons habilitados como `unauthorized` quando falta aprovação humana, inclusive para manifestos carregados por arquivo em `--addon-dir`.
- `forge events observability` e MCP `forge.events.observability` agora expõem `forge.event_observability_index.v1`, um índice materializado em SQLite a partir de `global_events`, com backfill/sync no migrate, fallback derivado para stores legados, filtros por workflow/tenant/node/Addon, buckets por tenant/workflow/node/Addon e agregados de severidade, categoria, duração, retries, waits, pressão de contexto e política de memória.
- `forge interactive home` e MCP `forge.interactive.home` agora expõem painéis estruturados para foco de workflows, task board operacional, schedule worker, timeline global de eventos, cost ledger e contexto/memória, além do resumo textual da TUI; isso dá ao operador humano e a agentes externos a mesma visão de lanes de workflow, handoffs prontos, checkpoints, interações humanas, artefatos, custos, waits/schedules e memória sem abrir cada workflow manualmente.
- `forge ops snapshot` agora emite `forge.ops.addon_view_renderers.v1`, classificando views de Addons em famílias seguras de renderer, normalizando fontes de dados, permissões, capabilities, risco de ações, anchors HTML, affordance TUI e `forge.ops.addon_view_interaction_state.v1` com estado/filtros/charts/forms/listas/timeline/canvas/documentos seguros; `POST /api/addon-renderer/event`, CLI `forge ops renderer-event` e MCP `forge.ops.addon_renderer_event` validam eventos contra `allowed_client_events`, exigem `addon_id` quando `view_id` for ambíguo e gravam `addon_renderer_client_event` na timeline do workflow; snapshots posteriores projetam `forge.ops.addon_view_runtime_state.v1` por workflow com último evento, ator, payload, hover, seleção, filtros, draft e refresh; o HTML do console já renderiza um formulário genérico por renderer seguro para registrar eventos; `forge interactive home` também resume os renderers disponíveis.
- `forge workflow update-goal` agora reparsa `forge.intent.v2` usando o operating context existente e o catálogo atual de Addons, persistindo deliverables/capabilities atualizados e retornando added/removed deliverables. O workflow ativo do Forge OS foi reprocessado para deixar de ser `support_only` e exigir relatório Markdown, evidência Telegram e auditoria final.
- Testes adicionados em `tests/forge_addon_architecture.rs`.

## Atualização 2026-06-11 — Harness E TUI Operacional

Estado verificado em `forge 0.4.177`, branch `main` alinhado com `origin/main`.

- Commits recentes publicados:
  - `44e2a23 feat: expose harness mode through mcp`;
  - `cd2e3e5 feat: show harness mode in interactive home`;
  - `8170c82 feat: add harness slash command`.
- `forge harness doctor --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` e MCP `forge.harness.doctor` retornam `forge.harness.doctor.v1`, uma auditoria read-only consolidando modo Forge-first, policy de projeto, status de shim, plano de wrapper, token-headroom e próximos comandos antes de entregar um CLI ao harness.
- `forge harness mode --output json` retorna `forge.harness.mode.v1`, modo efetivo `observe_only`, `forge_first=false`, fonte `default_observe_only`, caminho de configuração de projeto `.forge/harness.json` e cadeia de precedência explícita.
- MCP `forge.harness.mode` retorna o mesmo contrato, permitindo que agentes auditem a política de harness sem abrir terminal interativo e sem executar brains.
- `forge interactive home --output json` inclui `dashboard.harness_panel`, `dashboard.harness_mode_panel` e `dashboard.harness_doctor_panel`, então o operador vê o centro de harness/headroom, o modo Forge-first e a readiness completa do harness no painel inicial junto dos demais painéis operacionais.
- `forge interactive slash-commands --output json` inclui `/harness` e `/harness doctor`, comandos scriptable e de baixo risco equivalentes a `forge harness mode --output json` e `forge harness doctor --executor <executor> --shim-dir <dir> --project-root <project-root> --output json`.
- `forge interactive harness`, MCP `forge.interactive.harness` e `dashboard.harness_panel` adicionam a superfície operacional consolidada de harness/headroom para TUI/web/agentes: ela agrega modo efetivo, doctor, status de shim, plano de wrapper e prévia de token-headroom, com comandos granulares para `doctor`, `shim-status`, `wrap-plan`, `install-shims`, `exec`, `sessions` e `sync`.
- `forge patch review`, MCP `forge.patch.review` e `/patch review` adicionam o primeiro contrato read-only de revisão de diff: coletam `git diff --stat`, `git diff --check`, `git status --short`, resumo por arquivo, recomendação de aprovação e artifact `forge.patch_review.v1` sem editar arquivos.
- `forge interactive patch-workbench`, MCP `forge.interactive.patch_workbench` e `dashboard.patch_workbench_panel` adicionam a superfície operacional de file editing/diff review para TUI/web/agentes: ela mostra arquivos alterados, filtra arquivos internos do store Forge, resume diff check/stat e aponta para comandos permission-gated do lifecycle de patch.
- `forge interactive permissions`, MCP `forge.interactive.permissions` e `dashboard.permissions_panel` adicionam a superfície operacional de permissões/aprovações para TUI/web/agentes: ela agrega membership multi-tenant, permissões de Addons e human interactions sem criar uma regra paralela de autorização.
- `forge patch diff`, MCP `forge.patch.diff` e `/patch diff` adicionam um modelo read-only de navegação de diff multi-file com seleção por arquivo/hunk, linhas classificadas por contexto/adição/remoção e comandos de próxima/anterior navegação, persistindo `forge.patch_diff.v1`.
- `forge patch restore`, MCP `forge.patch.restore` e `/patch restore` adicionam a execução explícita e aprovada de restauração de arquivos a partir de um artifact de revert: exigem `--approved-by` e `--confirm-restore`, restauram apenas paths repo-relativos já capturados pelo artifact de apply e persistem `forge.patch_restore.v1` com snapshots antes/depois.
- `forge sessions lifecycle`, MCP `forge.session.lifecycle` e `/sessions lifecycle` adicionam controle audit-only de lifecycle para shell sessions conhecidas, registrando estados `opened`, `attached`, `detached`, `closed`, `failed` ou `abandoned` no timeline global sem executar child process. O lifecycle agora é ordenado: recibos carregam `previous_state`, `lifecycle_sequence` e `transition`, transições inválidas são recusadas antes do evento global, e `forge sessions`/MCP `forge.sessions` expõem `lifecycle_policy.allowed_next_states` com comandos de próxima transição. `forge sessions` também filtra por `--provider`, `--state` e `--readiness`, com equivalentes MCP `provider_id`, `lifecycle_state` e `readiness`, para UI/agentes trabalharem em uma lane de provider ou lifecycle sem filtrar todo o JSON no cliente. `forge sessions history --session <id>`, MCP `forge.session.history` e `/sessions history` agora retornam o histórico cronológico de uma sessão específica, separando eventos `shell_launch_planned` e `brain_session_lifecycle`, contadores, estado atual, policy e próximos comandos auditáveis.
- `forge milestone manifest --version 0.5 --output json` ainda retorna `promotion_decision.decision=fail`, bloqueado por `replacement_grade_cli` e `experimental_multimodal_runtime`. O `harness doctor`, o harness center, o `patch review`, o `patch diff`, o `patch restore`, o `interactive patch-workbench` e o `interactive permissions` avançaram a capacidade de CLI de substituição, mas ela continua em `groundwork` até existir UX mais completa de permissão e file editing, controles mais profundos de lifecycle de provider/session com histórico por sessão, e fluxos reais ponta a ponta de coding/research.

### Atualização 2026-06-11 — Evidência Multimodal Fixture-Only

- `forge multimodal benchmark-result --approved-by <operator> --confirm-fixture-only --output json` adiciona o primeiro artifact `forge.multimodal.benchmark_result.v1` para benchmark multimodal aprovado e fixture-only.
- MCP `forge.multimodal.benchmark_result` expõe o mesmo contrato para agentes, com `async_safe=true`, `mutates_workflow=false` e schema estável para anexar evidência a workflows/milestones.
- O resultado registra explicitamente `installs_performed=false`, `model_execution_performed=false`, `device_access_performed=false` e `network_access_performed=false`, além do guard check `no_camera_microphone_screen_or_input_access`.
- O milestone 0.5 continua bloqueado: essa evidência prova a fronteira segura fixture-only, mas ainda não substitui benchmark/demo real com runtime guard aprovado.
- `forge multimodal status --project-root <path>` e MCP `forge.multimodal.status` com `project_root` agora leem `.forge/multimodal.json` quando o arquivo declara `experimental_enabled` e `approved_by`; a configuração habilita apenas planejamento experimental, enquanto `guard --allow` continua obrigatório para qualquer permissão sensível.
- O gap do milestone multimodal foi reduzido: a feature flag explícita existe, mas ainda falta benchmark/demo real com runtime guard aprovado antes de promover a capacidade além de `groundwork`.

## Plano Incremental

### P0 — Contrato De Fonte Da Verdade

Status: iniciado.

- Manter este documento como mapa operacional.
- Criar relatório por ciclo com gaps, decisões, arquivos alterados e evidências.
- Não declarar conclusão sem auditoria requisito por requisito contra goal1/goal2/goal3.

### P1 — Addon Kernel

Status: catálogo, validação, package determinístico, marketplace local persistente com trust store e verificação Ed25519, fetch/import autorizado de packages para cache local com limite/hash opcional, sync de registry index JSON/YAML para importar múltiplos packages por policy, lockfile auditável de packages com hashes/capabilities/policy atual, enforcement opcional de lockfile em fetch/sync/install com `forge.addon_package_lock_enforcement.v1`, compatibility gates por `version_req`, Forge version, API versions, runtimes, features, plataforma e plano de migração/rollback em mudança major, workflow persistente de migração/rollback para mudanças major, lifecycle persistente com install/upgrade/downgrade/enable/disable/uninstall, autorização persistente de permissões, gates granulares por ferramenta/recurso/integração/action/tenant scope e índice materializado de capacidades implementados e expostos no MCP.

Próximos passos:

- Executar migrações/rollback reais por worker assinado com backup/restore efetivo e validação pós-execução.
- Evoluir fetch/cache/sync-registry/package-lock para registry remoto completo com mirrors, política administrável de atualização automática, migração de dados e rollback assistido.
- Levar os gates de permissão para enforcement real em execução externa, approval por tenant e policies administráveis por Addon.
- Mover builders especializados de workflow para registradores externos/assinados de Addon.

### P2 — Organizational Operating Context

Status: primeiro corte por arquivo de projeto, registry SQLite, memberships com papéis/permissões/ambientes, grants/denies customizados, expiração/janela temporal, comando administrativo de update, índice físico de recursos, auditoria de cobertura, policy gate audit/enforce e enforcement opt-in em creation/runtime/handoff/mutations implementados. Egresso externo vinculado a `workflow_id` passa pelo tenant policy do workflow antes de qualquer entrega, egresso externo sem workflow alvo passa pelo operating context do projeto, e `events timeline`/`events observability` aplicam `context:read` mais filtros tenant implícitos do projeto em modo enforce.

- Expandir gestão de memberships para múltiplos usuários por organização, ambientes reais e UX operacional.
- Integrar `tenant-policy --mode enforce` nos caminhos restantes de leitura sensível, demais listas globais e políticas administráveis; egresso externo com ou sem workflow alvo já bloqueia por `workflow:deliver` antes do transporte, e timeline/observability globais já bloqueiam/listam por `context:read` com operating context.
- Associar cada tabela operacional crítica diretamente a `organization_id`, `brand_id`, `product_id`, `user_id` e `channel_id`; `runs`, `task_leases` e `task_checkpoints` já têm chaves tenant físicas, índices tenant e backfill por workflow, mas o restante do schema operacional ainda precisa de cobertura direta.
- Eventos de workflow já recebem projeção tenant-aware via `forge.event_stream.v1` e agora também entram em `tenant_index`; falta enforcement físico obrigatório nos writes.
- Expandir o uso de brand identity, design system, tom de voz e políticas para heurísticas de decisão, adapters externos, validadores e UI operacional; o pacote de contexto e o contrato de persona já carregam esses dados.
- Bloquear resposta/planning sem contexto organizacional resolvido quando multi-tenant estiver habilitado.

### P3 — Event Engine Universal

Status: inbox global, roteamentos `start_workflow`, `continue_workflow`, `modify_workflow`, `pause_workflow`, `resume_workflow` e `complete_workflow`, descoberta declarativa de `event_adapters` por Addon com `permission_gate`, policy gate inicial em `events route`, worker single-pass `events scan`, loop local bounded `events worker`, contrato plan-only `events service-plan`/MCP para serviço gerenciado com lease/backoff/health/shutdown, execução bounded de worker via `events service-run`/MCP com `event_services` persistido e lease/heartbeat renovados por ciclo, execução bounded de webhook ingress via `events service-run --kind webhook_ingress`/MCP com heartbeats de progresso, `webhook_report` e health persistidos, recuperação explícita de serviços stale por `events services-recover`/MCP, recuperação opt-in de leases `worker` stale durante `runtime-reconcile`/`runtime-daemon`, supervisor bounded `events service-supervise`/MCP sobre `service-run` com restart/backoff/stop-file/health agregado, reconciler inicial `events runtime-reconcile`/MCP consumindo `forge.registry_workflow_runtime.v1`, inbox, leases e schedules para recomendar/executar worker supervisor ou reidratar cron/wait_until due por `scan_due`, daemon `events runtime-daemon`/MCP com lease própria em `event_services`, modo bounded ou contínuo, stop-file/idle-exit cooperativos e retenção limitada de ciclos, listener bounded `events webhook-ingress` para POST HTTP real com HMAC-SHA256 opcional por segredo em env, emissor genérico `events emit` para `http://`/`https://` egress com HMAC-SHA256 e Bearer por env ou credential-vault, e adapters Telegram de mensagem/documento/relatório com dry-run, entrega Bot API controlada por env ou credential-vault e artifact de evidência por workflow implementados. `continue_workflow` já cobre anexar evidência, checkpoint, resposta humana, conclusão de task pronta e drive de run; `webhook-ingress` já cobre POST JSON -> validação HMAC opcional -> inbox -> route opcional via adapter policy; `events emit` já cobre validação de adapter, permission gate, allowlist de host, HMAC/Bearer/bot_token por env ou credential-vault, tenant policy `workflow:deliver` antes do transporte com workflow alvo ou pelo operating context do projeto sem workflow alvo, envelope tipado, relatório de entrega, auditoria persistida na timeline global e `workflow_artifact` quando há workflow alvo.

- Transformar o daemon contínuo em serviço de produção com recuperação automática de processo; `runtime-daemon --continuous` já remove a dependência de `max-cycles`, mantém lease própria, ciclos auditáveis e rehydration de cron due quando `--scan-schedules` está ativo, `services-recover` já marca leases vencidas como `stale`, e `runtime-reconcile`/`runtime-daemon --recover-stale-services` já recuperam leases worker vencidas antes das recomendações, mas ainda não recriam processo automaticamente após queda.
- Normalizar eventos de chat, WhatsApp, Discord, email, SMS, voice, API, cron, Kafka, RabbitMQ, MQTT, database, file, sensor e telemetry sobre `forge.addon_event_adapters.v1`; webhook HTTP genérico já tem primeiro ingresso/egresso executável e Telegram já tem ingress/adapters de egresso com dry-run e entrega Bot API controlada.
- Ampliar `continue_workflow` com payload schemas mais específicos, validação de assinaturas por origem/canal além do HMAC genérico de webhook e adapters de transporte sem handlers específicos no Core; HMAC, Bearer, HTTP(S), Bot API Telegram e credential-vault direto para egresso já existem no egresso.
- Migrar conceitos úteis do ForgeFlow Telegram webhook para Addon/event adapter.

### P4 — Dynamic Workflow Engine Por Capacidades

Status: primeiro corte auditável implementado; `forge.capability_resolution.v1` expõe ativações de workflow extension, projeta `runtime_contracts` por capacidade/extensão, emite `capability_suggestions` para dependências ausentes encontradas em Addons conhecidos ou packages confiáveis já indexados no marketplace local, e pode sincronizar registries autorizados sob demanda em `addons resolve`/MCP antes de sugerir packages, registrando evidência em `registry_syncs`. `forge.addon_planner_registry.v1` lista registradores `planning_strategy`/`replanning_strategy` e separa builders first-party internos de planners externos por contrato runtime. `forge.addon_runtime_contract_policy.v1` faz o preflight de dispatch seguro para contratos runtime, `forge.addon_runtime_contract_dispatch.v1` persiste envelopes de dispatch para workers, `dispatch-planner` cria payload padrão `forge.addon_planner_dispatch_input.v1`, `run-dispatch` processa a fila com rechecagem de policy, builtin allow-listed e demarcação de runtime externo, `forge.addon_runtime_workers.v1` registra workers externos disponíveis por runtime/trust, `claim/complete` formalizam ownership, snapshot de identidade/chave no claim, verificação Ed25519 de assinatura, attestation e resultado hashado no ledger, `execute-dispatch` roda workers `local_process` allowlisted via JSON stdin/stdout e workers `external_api` por HTTP/HTTPS JSON controlado/allowlisted com auth Bearer/HMAC por env ou credential-vault, e `execute-planner` já executa `planning_strategy`/`replanning_strategy` externo com plano Core de referência, validação de resultado e auditoria de equivalência. O planner usa um registry interno capability-first para builders first-party, com guards textuais apenas para intents legados, e tasks genéricas de Addon recebem referências aos contratos runtime correspondentes em `context_requirements`. Builders first-party aparecem como contratos declarativos, e Addons externos podem anunciar planners/validators/executors/handoffs sem alterar Core.

- Continuar removendo heurísticas textuais residuais de análise de deliverables/risks e de policies que ainda não nascem de manifestos de Addon.
- Addons ainda precisam executar validators, executors e handoffs com o mesmo nível de auditoria especializada dos planners e precisam de workers WASM assinados reais; contratos declarativos, preflight de dispatch, ledger persistente, registry de workers, claim/completion, executor `local_process`, executor `external_api` HTTP/HTTPS com auth/secret injection, `execute-planner` e processamento seguro do Core já existem como manifesto/catalogação/fila/execução auditada.
- Substituir execução dos builders internos first-party por registradores de Addon externos/assinados quando a equivalência validada puder ser promovida por política administrável, com rollback e evidência de benchmark.
- Transformar registry sync sob demanda em política administrável por projeto/tenant, com allowlist de mirrors, atualização automática segura e UX operacional.
- Suportar subworkflows e especialistas temporários por capability.

### P5 — Persistent Workflow Runtime

Status: `forge.workflow_runtime.v1` formaliza `ephemeral_workflow` e `persistent_workflow` no root persistido do workflow, incluindo lifetime, flags persistente/efêmero, possibilidade de virar persistente e scale-to-zero policy. `forge.registry_workflow_runtime.v1` projeta esse contrato em `forge list`/Ops snapshot, com resumo persistente/efêmero, postura scale-to-zero e ação operacional por workflow, diferenciando wakeup por evento de `run_due_schedule`/`sleep_until_schedule`. `forge events scan` executa uma passagem bounded de inbox, `forge events worker` executa loop local bounded para event loop operacional com `stop_file` cooperativo, `forge events service-plan` cria contrato auditável de serviço gerenciado para worker/webhook, `forge events service-run --kind worker` já persiste lease/heartbeat/health/status por ciclo enquanto executa o worker bounded e para com status `stopped` quando o stop file é observado, `forge events service-run --kind webhook_ingress` já executa o listener bounded com serviço persistido e shutdown cooperativo entre requests, `forge events services-recover` já marca leases vencidas como stale com evidência, `forge events runtime-reconcile --recover-stale-services` e `forge events runtime-daemon --recover-stale-services` já aplicam essa recuperação automaticamente nos ciclos de runtime, `forge events service-supervise` já fornece restart/backoff/health agregado bounded sobre esses serviços, `forge events runtime-reconcile` já conecta ação operacional do registry, inbox, leases e schedule worker status para recomendar/executar worker supervisor ou `scan_due`, `forge schedule scan-due` já executa cron e `wait_until` one-shot sob lease, e `forge events runtime-daemon --scan-schedules` já reidrata ambos em ciclos com lease/heartbeat próprios, modo contínuo, retenção limitada de ciclo e contadores agregados de schedule.

- Evoluir `runtime` para controlar execução real de waits longos genéricos além de `wait_until` one-shot, leases, rehydration e daemon workers contínuos.
- Fazer os próximos workers de espera consumirem a ação operacional do registry, como já ocorre na separação atual entre ações de evento e ações de schedule.
- Garantir espera indefinida por eventos, scale-to-zero, resume e goal mutation.
- Evoluir `events runtime-daemon`, `events service-supervise` e `schedule scan-due` para daemon/worker reidratável de produção com recuperação após queda de processo, espera longa, decisão automática de escala e integração com waits não-cron além do inbox.

### P6 — TUI E Ops Console De Classe Mundial

Status: ops console local, visual workspace, descoberta de views por Addon e observabilidade consolidada de Addons iniciados; `forge.addon_views.v1` lista contribuições de UI/TUI/ops-console por Addon/surface/lifecycle com contrato de tipo, componente, layout, bindings, ações e props, `forge.ops.addon_view_renderers.v1` classifica essas views em famílias seguras (`dashboard_renderer`, `visualization_renderer`, `editor_renderer`, `data_list_renderer`, `timeline_renderer`, `canvas_renderer`, `document_renderer`), normaliza fontes de dados/permissões/capabilities/risco de ações, adiciona `forge.ops.addon_view_interaction_state.v1` com estado Forge-owned, filtros, hover de chart, forms, sort/paginação, cursor de timeline, palette de canvas e outline de documentos, e bloqueia props inseguras sem executar componente externo; web `POST /api/addon-renderer/event`, CLI `forge ops renderer-event` e MCP `forge.ops.addon_renderer_event` validam eventos de cliente contra o contrato do renderer, exigem `addon_id` em colisão de `view_id`, persistem `addon_renderer_client_event` na timeline do workflow e o snapshot recompõe `forge.ops.addon_view_runtime_state.v1` por workflow; `forge.addon_observability.v1` resume lifecycle, permission gates, eventos e dispatch usage, e `forge ops snapshot|serve --addon-dir ...` já consome views `ops_console` habilitadas como cards de composição operacional/renderers seguros, estado interativo, estado persistido, formulário genérico de eventos e tabela de "Observabilidade de Addons". O `forge interactive home` também expõe `workflow_focus`, `ui_composition_panel`, `task_board_panel`, `schedule_panel`, `event_panel`, `structured_logs_panel`, `cost_panel`, `context_memory_panel` e `addon_renderer_panel` por CLI/MCP; o `ui_composition_panel` usa `forge.interactive.ui_composition.v1` para declarar regiões ordenadas, widgets Core, widgets de Addons seguros, famílias de renderização e comandos de refresh/inspeção sem acoplar domínio, o `structured_logs_panel` usa `forge.interactive.structured_logs.v1` para expor eventos recentes com sequência, workflow, tipo, categoria, severidade, origem, correlação, observabilidade e preview de payload sem parsing de string, enquanto o `task_board_panel` usa `forge.interactive.task_board.v1` para mostrar lanes, cards operáveis por tarefa, contagem de tasks por estado, handoffs prontos, checkpoints de retomada, interações humanas pendentes, artefatos e próximos comandos operacionais. As mesmas superfícies agora existem diretamente em `forge interactive task-board`/MCP `forge.interactive.task_board`, `forge interactive workflow-dag`/MCP `forge.interactive.workflow_dag` e `forge interactive structured-logs`/MCP `forge.interactive.structured_logs`, permitindo renderização dedicada por TUI/web/agentes sem carregar todo o home.

- Expandir o `task_board_panel` para navegação interativa real no terminal/web, com drill-down de DAG, handoffs, checkpoints, approvals e artifacts em tempo real.
- Task board humano+IA.
- UI composition por Addons já possui primeira projeção segura por famílias com contrato de estado/filtros/charts/forms/listas/timeline/canvas/documentos sem executar código arbitrário do Addon, rota persistente de evento de cliente, comando CLI/MCP, formulário HTML genérico e projeção de estado por workflow em snapshot; ainda precisa transformar isso em TUI/web runtime com navegação, edição persistente rica e sincronização incremental de estado de cliente.
- Levar a composição para uma TUI interativa real e expandir whiteboard, tokens, components e artifact editors como views dinâmicas editáveis.

### P7 — Memory, Identity E Governance

Status: política e busca file-first implementadas para níveis de memória, escopos global/organização/projeto/processamento, root físico de organização, visibilidade e shareability. Pacotes de contexto e handoff agora carregam `forge.context.memory_policy.v1`, derivado do operating context do workflow/tenant, incluindo o comando governado `forge memory search --workflow <workflow-id>`. `forge memory search/promote/promotions/retention/cleanup` e MCP `forge.memory.search/promote/promotions/retention/cleanup` aceitam `--workflow`/`workflow_id`; com `tenant_policy_mode: enforce`, o Forge deriva a organização do workflow, bloqueia organização divergente e rejeita escopos fora do `memory_scope` antes de ler/escrever. `forge memory promote`/`forge.memory.promote` implementam promoção curada e auditável para project/organization/global com aprovação, e `forge memory promotions`/`forge.memory.promotions` expõem o índice SQLite dessas trilhas com colunas físicas workflow/organization/brand/product/user/channel e filtros tenant por workflow. `forge memory retention`/`forge.memory.retention` avaliam retenção/expiração sem apagar arquivos. `forge memory cleanup`/`forge.memory.cleanup` executam arquivamento/deleção apenas após approval, reason e confirm, e só para processing memory classificada como `delete_after_final_packaging`. Identidade unificada cross-channel agora tem `identity_links` persistido, CLI/MCP para link/unlink/list/resolve e integração operacional com `tenant-policy`.

- Evoluir merge/separate avançado de perfis, detecção de conflitos, consentimento por organização e UX operacional para revisão humana de vínculos de identidade.

### P8 — Observability E Cost OS

Status: `forge.event_timeline.v1` implementado como timeline global tenant-aware sobre a tabela append-only `global_events`, cobrindo eventos de workflow e eventos inbound antes de existir workflow, com fallback legado para stores antigos, filtros por tenant e cursor `--after-sequence`/`page.next_cursor`; em `tenant_policy_mode: enforce`, `events timeline --project-root ...` e MCP `forge.events.timeline` exigem `context:read`, aplicam organization/brand/product do operating context quando filtros são omitidos e rejeitam filtros explícitos fora do tenant antes de retornar dados globais. Envelopes de stream/timeline agora incluem `forge.event_observability.v1` para projetar node/task, Addon, duração, retry, wait, pressão de contexto e política de memória quando essa evidência já existe no payload bruto; `forge.event_observability_index.v1` implementado como índice materializado em SQLite por workflow/tenant/node/Addon, com backfill/sync a partir de `global_events`, fallback derivado para stores legados, buckets, severidade/categoria e agregados de duração/retry/wait/contexto/memória; `events observability --project-root ...` e MCP `forge.events.observability` também exigem `context:read`, aplicam filtros tenant implícitos do projeto e bloqueiam filtros fora do tenant em modo enforce. `forge.event_observability_history.v1` implementado como rollup histórico derivado do índice materializado, com buckets `hour|day`, agrupamento `none|tenant|workflow|node|addon`, metadados estruturados de `group` por bucket e enforcement tenant via `context:read` em CLI/MCP quando `tenant_policy_mode: enforce`; `forge.event_improvement_policy.v1` implementado como primeira camada read-only tenant-safe de política automática sobre observabilidade, exigindo `context:read` e aplicando filtros tenant implícitos antes de recomendar node determinístico, reparo de context routing, rework/validation gate ou supervisão de waits a partir de thresholds ajustáveis, `forge improve` incorpora as principais recomendações ao artifact/changelog de experimento controlado sem autopromoção, `forge improve apply-event-policy` aplica recomendações em escopo node/Addon/workflow como revisão governada com aprovação, changed fields, rollback snapshot e gate de equivalência pendente para `prefer_deterministic_node`, `add_validation_or_rework_gate`, `tighten_context_routing` e `supervise_wait_or_external_dependency`, `forge improve benchmark-event-policy` emite `forge.improve.event_policy_benchmark.v1` para validar a última aplicação contra estado atual, rollback readiness e `validate_workflow`, gravando evidência de benchmark sem autopromover, e `forge improve promote-event-policy` emite `forge.improve.event_policy_promotion.v1` para aceitar somente benchmark validado com aprovação explícita e gravar revisão/evento idempotente mantendo `auto_promoted=false`; `forge.cost_ledger.v1` implementado como ledger read-only tenant-safe por workflow/node/tenant e origem de Addon detectável, exigindo `context:read` e aplicando filtros tenant implícitos em modo enforce antes de combinar custo estimado de tasks e custo/tokens observados em eventos; `forge.cost_ledger_index.v1` implementado como materialização normalizada tenant-safe em SQLite com linhas `planned_task` e `observed_event` por workflow/tenant/Addon/executor/model-call flags, exigindo `context:read`, aplicando filtros tenant implícitos antes de escrever/retornar linhas e rejeitando filtros fora do tenant; `forge.cost_ledger_incremental.v1` implementado como materialização incremental tenant-safe por cursor de `global_events`, exigindo `context:read`, aplicando filtros tenant implícitos antes do scan, deduplicando apenas workflows afetados do tenant e rejeitando filtros fora do tenant; `forge.cost_ledger_history.v1` implementado como rollup histórico tenant-safe por `hour|day` a partir do índice materializado, exigindo `context:read`, aplicando filtros tenant implícitos em modo enforce e agrupando por `none|tenant|workflow|source_kind|addon|executor`; `forge.cost_ledger_maintenance.v1` implementado como recibo idempotente tenant-safe que exige `context:read`, aplica filtros tenant implícitos antes de materializar o índice e retorna rollup histórico com retenção plan-only para execução periódica segura; `forge.cost_ledger_daemon.v1` implementado como loop dedicado bounded tenant-safe sobre a manutenção de custos, exigindo `context:read`, aplicando filtros tenant implícitos em cada ciclo e registrando cada ciclo na timeline global como `cost_ledger_daemon_cycle` sob o tenant efetivo; `forge.cost_ledger_retention.v1` implementado como retenção física approval-gated tenant-safe para linhas antigas do índice normalizado, exigindo `context:read`, aplicando filtros tenant implícitos antes de listar/deletar candidatos e rejeitando filtros fora do tenant; `forge.addon_observability.v1` implementado como visão consolidada de Addons, lifecycle, capability/resource/event/UI contracts e dispatch usage, exposta por CLI/MCP e pelo Ops snapshot/HTML; MCP `forge.events.timeline`, `forge.events.observability`, `forge.events.observability_history`, `forge.events.improvement_policy`, `forge.improve.apply_event_policy`, `forge.improve.benchmark_event_policy`, `forge.improve.promote_event_policy`, `forge.cost.ledger`, `forge.cost.materialize`, `forge.cost.incremental`, `forge.cost.history`, `forge.cost.maintain`, `forge.cost.daemon`, `forge.cost.retention` e `forge.addons.observability` expostos.

- Global events já materializa observabilidade em tabela própria com pressão de contexto/memória por node/Addons, rollups históricos por bucket e recomendações read-only de política de melhoria; `forge improve` consome essas recomendações no experimento controlado, `apply-event-policy` aplica policies em escopo node/Addon/workflow com aprovação/rollback/equivalência pendente, `benchmark-event-policy` valida equivalência estrutural/rollback/validação sem autopromoção e `promote-event-policy` grava aceitação governada após benchmark aprovado.
- Cost ledger agora materializa histórico normalizado em `cost_ledger_index`, expõe rollups por período em `forge.cost_ledger_history.v1`, tem `forge cost incremental`/MCP para agregação por cursor de eventos, `forge cost maintain`/MCP para backfill idempotente agendável, `forge cost daemon`/MCP como loop dedicado bounded com auditoria em `global_events` e `forge cost retention`/MCP para deleção física somente com approval/reason/confirm.
- Tokens, tempo, retries, wait time, memory/context pressure.
- Detecção inicial de trabalho repetitivo, retries, waits e pressão de contexto existe em `forge events improvement-policy`, entra no artifact/changelog de `forge improve`, pode ser aplicada por fluxo controlado em escopo node/Addon/workflow, gera recibo de benchmark/equivalência antes de promoção e já grava a aceitação governada após aprovação humana explícita.

### P9 — ForgeFlow Alpha Absorption

- Migrar Telegram ingress como Addon oficial.
- Migrar runtime web/API/Postgres como deployment profile ou runtime adapter.
- Criar relatório de equivalência: o que do alfa foi absorvido, substituído ou descartado.

## Gatilhos De Aceite Por Fase

- Addon Kernel pronto quando um domínio novo entra via manifesto externo e `forge plan` usa suas capacidades sem alteração de Core.
- Multi-tenant pronto quando workflows/artifacts/events/memory não existem sem organization boundary em modo multi-tenant.
- Event engine pronto quando eventos externos conseguem iniciar e modificar workflows sem handler específico no Core.
- Dynamic workflow pronto quando planejamento/replanejamento vem de capability graph + strategies, não de branch hard-coded no Core.
- TUI pronta quando operação humana consegue acompanhar, pausar, retomar, modificar e auditar workflows pela interface.
- ForgeFlow absorvido quando a funcionalidade útil do alfa existir como Core contract ou Addon, sem dependência estrutural do alfa.
