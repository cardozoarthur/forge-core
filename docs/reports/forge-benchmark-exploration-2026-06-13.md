# Forge Benchmark Exploration Report

Data: 2026-06-13

Este documento consolida o que deve ser absorvido de cada benchmark que apareceu na direção do Forge. O objetivo não é copiar produto nem código. É transformar padrões úteis em contratos Forge-owned.

## Método

Leitura combinada de:

- docs e código locais do Forge Core;
- documentação oficial e páginas públicas dos benchmarks;
- GitHub/docs quando o benchmark é open source;
- páginas de produto, help e comunidade quando a UX é mais importante que o código.

## Corte De Arquitetura

O inventário foi separado em três classes:

- Core benchmarks: definem roteamento, execução, resume, checkpoints, brain selection e o TUI principal.
- Addon-first benchmarks: definem experiências e domínios opcionais, como design, memória de arquivos, documentos e know-how local.
- Dual-use benchmarks: influenciam tanto o Core quanto Addons porque moldam a própria visualização de workflow, grafos, nodes e automação externa.

Também vale uma regra transversal:

- criação de arquivo é workflow, não write cru;
- o workflow deve inspecionar dados, normalizar, validar, renderizar, persistir e anexar artefato;
- isso vale para relatório, config, export, seed e sessão.

Benchmarks dual-use que exigem leitura cuidadosa:

- n8n: informa o Core de workflow graph e a superfície Addon de automação/interoperabilidade.
- Obsidian: informa tanto o Core de contexto ligado quanto o Addon de canvas, backlinks e navegação local-first.

## More Benchmarks That Shape The Contract

### Remotion

Remotion is useful as a benchmark for artifact-first pipelines: a reusable template, parameterized inputs, previewable composition and renderable output. The Forge adaptation is not to become a video tool; it is to preserve the same discipline for any generated artifact.

Useful source signals:

- reusable templates are the primitive;
- composition is parameterized, not hard-coded;
- preview and render are separate steps;
- docs explicitly position the tooling for AI/automation use.

What to absorb:

- reusable artifact templates;
- parameters over hard-coded output;
- preview before render;
- production of durable artifacts from structured inputs;
- an explicit separation between authoring and rendering.

Placement:

- Addon-first, with artifact contracts in Core.

### headroom

Headroom is a benchmark for context economy. Its value is not a feature list; it is the discipline of compressing or projecting context before the model sees it.

Useful source signals:

- it compresses tool outputs, logs, files and conversation history;
- the output stays semantically equivalent while token cost drops;
- OpenClaw-specific plugin support shows the pattern can be applied per executor/surface;
- the tool is effectively a context gateway, not just a summarizer.

What to absorb:

- reversible context compression;
- wrapper interception;
- token budget awareness;
- short retrieval refs instead of dumping whole files;
- choosing what to show the brain versus what to keep in storage.

Placement:

- Core, because context policy belongs in the router and harness.

## Benchmarks E O Que Absorver

### 1. Codex

Fonte de referência:
- execução local pragmática;
- separação entre `exec` e `review`;
- `resume` simples;
- sandbox e aprovação visíveis;
- foco em produção de código/testes.

O que o Forge deve absorver:
- brain de execução substituível sob autoridade do Forge;
- `resume` por código único de chat/run;
- política explícita de sandbox e aprovação;
- comandos de execução e revisão como fluxos distintos;
- contexto curto e acionado, não contexto despejado.

Placement:
- Core.

### 2. Gemini CLI

Fonte de referência:
- experiência interativa primeiro;
- sessão persistente;
- modo `--prompt` headless;
- aprovação e raw output como política visível;
- sensação shell-first.

O que o Forge deve absorver:
- TUI de entrada simples por padrão;
- resume de sessão como contrato real;
- shell mode separado do chat normal;
- política de aprovação mostrada como estado, não como hidden magic.

Placement:
- Core.

### 3. OpenCode

Fonte de referência:
- TUI como entrypoint padrão;
- sessão continue/fork;
- agentes e providers visíveis;
- modo run/headless;
- UX compacta e de operador.

Observed surfaces worth copying as contracts, not code:

- `opencode` opens the TUI by default;
- `--continue`, `--session` and `--fork` are first-class session controls;
- `--prompt`, `--command` and `--format` separate headless invocation from the interactive loop;
- `opencode serve` exposes a headless HTTP server for the same workflow graph;
- primary agents and model/provider selection stay visible to the operator.

O que o Forge deve absorver:
- `forge` abre a TUI diretamente;
- o orquestrador decide entre responder direto e criar workflow;
- `Tab` e sugestões devem ser contextuais, não poluição visual;
- visão compacta com comandos sob demanda;
- alternativa web/braço visual pode vir depois como ponte.

Placement:
- Core.

### 4. Claude CLI

Fonte de referência:
- fluxo longo;
- sensação de assistência persistente;
- handoffs e mudanças durante a execução.

Observed surfaces worth copying as contracts, not code:

- session naming appears in `/resume` and terminal title;
- `--resume <name>` restores a named session;
- plan mode exists as a visible approval boundary before editing;
- the agent remembers files read, analysis done and conversation history across resumes;
- cloud/web sessions can resume in desktop/editor contexts.

O que o Forge deve absorver:
- workflows long-lived com mutação enquanto rodam;
- checkpoints, waits e resume sem perder lineage;
- mudanças de objetivo sem recomeçar do zero.

Placement:
- Core.

### 5. LangGraph

Fonte de referência:
- grafo como unidade de execução;
- `thread_id` e checkpoints como identidade durável;
- subgraphs reutilizáveis;
- interrupt / resume / time-travel;
- estado explícito.

O que o Forge deve absorver:
- todo workflow como grafo persistente;
- subworkflow como unidade reutilizável;
- resume por identidade estável;
- estado visível e mutável durante o run;
- chamadas assíncronas e paralelismo quando houver espaço.

Placement:
- Core.

### 6. LangChain

Fonte de referência:
- `create_agent` como harness mínimo;
- middleware como primitive;
- tool handling e context engineering;
- guardrails e summarization como composição.

Observed surfaces worth copying as contracts, not code:

- middleware is the main composition primitive;
- context engineering is explicit and step-level;
- tool handling and error handling are contract-backed rather than ad hoc;
- summarization, permissioning and state shaping are reusable layers.

O que o Forge deve absorver:
- brain routing e tool selection com middleware explícito;
- contexto curto, selecionado e com políticas;
- guardrails de execução como camada modular;
- tratamentos de ferramenta e falhas por contrato.

Placement:
- Core.

### 7. OpenClaw

Fonte de referência:
- operação assíncrona;
- superfícies multi-canal;
- colaboração persistente;
- handoff durável.

O que o Forge deve absorver:
- canais como Addons ou adaptadores;
- painéis diferentes para os mesmos workflows;
- interação assíncrona com estado persistente;
- operação humana e IA em superfícies diferentes.

Placement:
- Addon-first.

### 8. Hermes

Fonte de referência:
- memória file-first;
- busca semântica por conteúdo;
- scopes de memória;
- promoção e retenção governadas.

O que o Forge deve absorver:
- memória global, de projeto e de processamento separadas;
- `.forge` como memória de projeto;
- processamento temporário quando fizer sentido;
- busca semântica por arquivo como contrato nativo.

Placement:
- Addon-first, com parte do contrato no Core.

### 9. Open Design

Fonte de referência:
- fluxo de artefato e visual;
- produtos visuais e layouts como objetos de trabalho.

O que o Forge deve absorver:
- whiteboard, wireframe, flow e tokens como artefato Forge;
- visualização de projeto e documentação como workflow.

Placement:
- Addon-first.

### 10. Penpot

Fonte de referência:
- design tokens;
- componentes;
- colaboração em design system;
- layout e organização visual.

O que o Forge deve absorver:
- tokens e componentes como objetos versionáveis;
- páginas e fluxos visuais como workflow;
- visão de sistema de design, não só tela isolada.

Placement:
- Addon-first.

### 11. n8n

Fonte de referência:
- trigger/action graph;
- schedules;
- nodes;
- marketplace de nodes;
- UX visual de automação.

O que o Forge deve absorver:
- workflow orientado a eventos e triggers;
- schedules e handoffs como nós de primeira classe;
- no-code/low-code sem perder governança;
- capacidade de descobrir/reusar partes já existentes;
- uma UI de workflows que pode aprender da densidade visual do n8n;
- nodes Forge que também possam falar com o ecossistema n8n.

Placement:
- Core + Addon-first.

### 12. Paperclip

Fonte de referência:
- processamento documental;
- secure client folders;
- straight-through processing;
- queues e audit trail;
- encryption-in-use;
- data digitization com fluxo operacional.

O que o Forge deve absorver:
- criação de arquivo/documento como workflow completo;
- roteamento de documentos com validação e fila;
- trilha de auditoria e segurança;
- transformar dados brutos em artefatos estruturados;
- UX de operação empresarial, não só de CLI técnica.

Placement:
- Addon-first, com contratos de workflow no Core.

### 13. Obsidian

Fonte de referência:
- local-first notes;
- backlinks;
- graph view;
- canvas visual;
- plugins e extensões;
- navegação entre notas e artefatos.

O que o Forge deve absorver:
- contexto local e notas ligadas como superfície de operação;
- visualização de relacionamentos entre artefatos;
- canvas e graph para tarefas, workflows e documentos;
- plugin model como referência para capabilities opcionais.

Placement:
- Addon-first.

### 14. OpenSquad

Fonte de referência:
- colaboração multiagente visível;
- quadros de trabalho e handoffs entre participantes;
- coordenação humana + IA com visibilidade do estado;
- transcrição ou transporte entre agentes como fluxo explícito.

O que o Forge deve absorver:
- quadro de tarefas e sub-tarefas com estado claro;
- handoff visível entre agentes e humanos;
- multi-agent orchestration sem esconder o fluxo;
- paralelismo com ownership explícito;
- possibilidade de usar a mesma workflow graph em diferentes superfícies.

Placement:
- Dual-use, porque inspira tanto a estrutura do workflow quanto a UI colaborativa.

### 15. superpowers

Fonte de referência:
- brainstorming antes de construir;
- debugging sistemático;
- TDD;
- verification-before-completion;
- trabalho com worktrees;
- disciplina de processo.

Observed surfaces worth copying as contracts, not code:

- brainstorming before writing code or adding behavior;
- root-cause-first debugging;
- explicit verification before completion;
- worktree isolation for implementation;
- process skills before implementation skills.

O que o Forge deve absorver:
- workflow gates para planejar, debugar, verificar e então promover;
- disciplina operacional imposta pelo runtime;
- contexto e paralelismo com critério, não por impulso.

Placement:
- Core.

### 16. Installed skills and plugins

Fonte de referência:
- capability packs;
- procedimentos reutilizáveis;
- instruções estruturadas por tarefa.

O que o Forge deve absorver:
- skills viram workflows, context packs ou adapters;
- comportamento não fica escondido em texto solto;
- capability registry precisa ser consultável e versionado.

Placement:
- Core, com extensões Addon.

### 17. credential-vault

Fonte de referência:
- acesso seguro a segredos;
- injeção controlada no terminal;
- contrato local visível.

Observed surfaces worth copying as contracts, not code:

- secrets are accessed through a brokered path, not stored in chat;
- the vault has a visible contract and a local encrypted store;
- terminal injection is controlled, which means workflows ask for secrets through the vault boundary instead of depending on ambient env;
- secret access is a workflow dependency, not a hidden side effect.

O que o Forge deve absorver:
- segredos nunca aparecem em chat/log;
- acesso por canal controlado;
- credenciais como dependency contract do workflow.

Placement:
- Core de segurança.

### 18. telegram-delivery

Fonte de referência:
- entrega de artefato e mensagem final;
- message_id e document_message_id como recibo;
- handoff depois de verificação.

Observed surfaces worth copying as contracts, not code:

- completion handoff includes both message and document evidence when applicable;
- delivery is attached to the workflow outcome;
- returned ids are audit evidence;
- this should remain optional and workflow-owned.

O que o Forge deve absorver:
- entrega final é parte do workflow;
- relatório e artefato são saídas governadas;
- handoff deve retornar ids auditáveis.

Placement:
- Addon/workflow delivery.

## UX/UI Que Vale Reaproveitar

- OpenCode: tela única limpa, comandos sob demanda, shell-first.
- Gemini: sessão interativa persistente, resume simples, modo de comando.
- Obsidian: backlinks, canvas, graph e local-first notes.
- n8n: nós e conectores visuais, triggers e schedules muito claros; também serve como referência para a UI central de workflows do Forge.
- OpenSquad: quadros de colaboração, handoffs visíveis e multi-agent orchestration.
- Paperclip: filas, documentos, triagem e trilha de auditoria.
- Penpot/Open Design: artefatos visuais versionáveis.

## API E Código Que Vale Reaproveitar Como Inspiração

Sem copiar implementação:

- LangGraph: grafo, estado, checkpoint e resume.
- LangChain: middleware, ferramentas e context engineering.
- OpenCode: CLI/TUI como superfície principal.
- n8n: node graph com triggers e schedules; também inspira a visualização nativa de workflows do Forge e a interoperabilidade com automação externa.
- Obsidian: plugin/canvas/backlink model.
- OpenSquad: shared workflow graph com colaboração visual.

## O Que O Forge Deve Fazer Com Isso

1. manter Core pequeno e universal;
2. tratar workflows como unidade central;
3. tratar criação de arquivo como workflow completo;
4. separar Core de Addon-first de forma explícita;
5. manter resume e chat-code como contratos públicos;
6. manter SDKs por linguagem consumindo o mesmo contrato;
7. manter installer único, sem semântica diferente por plataforma;
8. continuar permitindo brain substituível sem perder governança.

## Links De Referência

- Obsidian: https://obsidian.md/
- Obsidian Canvas: https://obsidian.md/canvas
- Obsidian Backlinks: https://obsidian.md/help/plugins/backlinks
- Obsidian Community: https://community.obsidian.md/
- Paperclip: https://paperclip.com/
- Paperclip SAFE: https://paperclip.com/safe-solutions-overview/
- Paperclip Mojo: https://paperclip.com/mojo/
- Paperclip VCF: https://paperclip.com/vcf-solutions-overview/
- n8n Docs: https://docs.n8n.io/
- n8n Schedule Trigger: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.scheduletrigger/
- LangGraph docs: https://docs.langchain.com/oss/python/langgraph/graph-api
- LangChain docs: https://docs.langchain.com/oss/python/langchain/overview
- OpenCode GitHub: https://github.com/anomalyco/opencode
- OpenCode CLI docs: https://github.com/anomalyco/opencode/blob/dev/packages/web/src/content/docs/cli.mdx

## Conclusão

O Forge não deve copiar nenhum desses sistemas. Ele deve absorver:

- a UX limpa de terminal;
- o grafo de workflow resumível;
- o tratamento sério de documentos e arquivos;
- a navegação local-first de conhecimento;
- o processo disciplinado de skills e automação;
- a separação correta entre Core e Addon.

O resultado esperado é um sistema em que tudo importante vira workflow, inclusive arquivo, documento, conhecimento, execução e entrega.
