# Forge 0.5 - Relatório de Status

Data: 2026-05-25 18:09 -03  
Repositório: `/home/arthur/projects/forge-core`  
Versão atual instalada: `forge 0.4.113`  
Workflow de autoevolução: `wf_047a8146d7fb42a7800cbfdad1b59f72`

## Resumo Executivo

A versão `0.5` ainda **não está pronta para promoção**.

O Forge já validou boa parte da infraestrutura necessária para a 0.5, principalmente:

- CLI interativo;
- comandos por `/`;
- roteamento conversacional;
- decisões humanas/formulários;
- base de scheduler/loops/subflows;
- IR criativo inicial;
- tokens de design;
- componentização;
- superfície MCP para artifacts criativos e tokens.

Mas ainda faltam três blocos essenciais para chamar isso de Forge `0.5`:

- colaboração ao vivo real;
- pesquisa técnica consolidada dos sistemas de referência;
- demos/exportações reais de workflows criativos.

Status formal da milestone:

| Status | Quantidade |
|---|---:|
| Validado | 6 |
| Groundwork | 1 |
| Planejado | 2 |
| Bloqueado | 0 |
| Total | 9 |

Decisão atual de promoção: **fail**.  
Motivo: ainda existem capacidades planejadas ou apenas groundwork.

## O Que Já Foi Feito

### 1. CLI Interativo do Forge

Status: **validado**

Já existe uma base para o Forge funcionar como CLI interativo próprio.

Evidências:

- `forge` em TTY abre uma home interativa com marca/anvil e dashboard;
- `forge interactive home`;
- `forge interactive slash-commands`;
- `forge interactive route --input <texto>`;
- modo sem TTY continua seguro para scripts;
- roteamento conversacional distingue resposta direta, comando `/` e workflow assíncrono;
- decisões de retenção de workflow já aparecem como resultado estruturado.

Limite atual:

- ainda falta um TUI completo com loop interativo rico, autocomplete e edição fluida.

### 2. Comandos por `/` e Roteamento Conversacional

Status: **validado**

O Forge já tem catálogo de comandos por `/`, incluindo comandos como:

- `/help`;
- `/status`;
- `/list`;
- `/inspect`;
- `/runs`;
- `/workflows`;
- `/artifacts`;
- `/costs`;
- `/config`;
- `/sync`;
- `/executors`;
- `/runtimes`;
- `/validate`;
- `/approve`;
- `/reject`;
- `/goal`;
- `/attach`;
- `/resume`;
- `/pause`;
- `/stop`;
- `/delete`;
- `/export`;
- `/logs`;
- `/update`.

O Forge também já consegue classificar entrada humana como:

- resposta direta sem criar workflow;
- comando operacional;
- nova execução assíncrona com `workflow_id` e `run_id`.

### 3. Decisões Humanas e Formulários

Status: **validado**

Já existe modelo de interação humana persistente.

Evidências:

- choice prompts;
- formulários com campos obrigatórios;
- validação de formulário;
- decisão durável com timestamp, origem e rationale;
- pausa de workflow quando decisão humana é necessária;
- retomada após resposta;
- timeout sem bypass;
- visibilidade em `status`, `list` e `inspect`;
- ponte MCP para criar/listar/responder/expirar interações.

Limite atual:

- ainda falta a experiência rica no TUI/web;
- ainda falta transformar respostas repetidas em defaults/policies com aprovação explícita.

### 4. Scheduler, Loops e Subflows

Status: **validado**

Essa base está bem mais madura.

Evidências:

- cron/schedule como grafo;
- loop state;
- due execution;
- missed-run reconciliation;
- scale-to-zero;
- scan-due;
- schedule list;
- schedule summary;
- loop summary;
- smoke de daily Goal research gerando Markdown, PDF e delivery record Telegram redatado;
- DAG parallel scheduling com waves concorrentes.

Limite atual:

- ainda faltam adapters produtivos para pesquisa real com navegador/dados externos dentro do runtime final.

### 5. IR Criativo Inicial

Status: **validado**

O Forge já tem base de artifact criativo estruturado.

Tipos já modelados:

- screen;
- whiteboard;
- document;
- slide deck;
- component.

Evidências:

- `CreativeArtifact`;
- `ScreenSpec`;
- `WhiteboardSpec`;
- `DocumentSpec`;
- `SlideDeckSpec`;
- `ComponentSpec`;
- round-trip via serde;
- attach/list/inspect por CLI;
- integração no workflow.

Limite atual:

- ainda falta renderização real;
- ainda falta edição visual;
- ainda falta import/export declarativo;
- ainda falta patch-by-intent executável em artifacts reais.

### 6. Design System e Tokens

Status: **validado**

Já existe modelo persistente de tokens.

Evidências:

- `DesignToken`;
- `TokenType`;
- `TokenCollection`;
- `SemanticAlias`;
- `workflow set-tokens`;
- `workflow get-tokens`;
- ferramentas MCP `forge.tokens.get` e `forge.tokens.set`.

Limite atual:

- ainda falta engine de resolução de tokens;
- ainda falta herança/overrides real;
- ainda falta propagação global;
- ainda falta demo provando que alterar token muda o design inteiro preservando edições humanas.

### 7. Componentização AI-first

Status: **validado**

Já existe base de manifesto de componente.

Evidências:

- props;
- variants;
- states;
- slots;
- token dependencies;
- code template;
- `PatchByIntent` schema;
- CLI/MCP já conseguem interagir com artifacts criativos.

Limite atual:

- ainda falta preview renderizado;
- ainda falta action registry completo;
- ainda falta execução real de patch-by-intent;
- ainda falta geração de componente por IA com validação visual/estrutural.

### 8. MCP Criativo e Tokens

Status: **groundwork**

No ciclo 28, o Forge adicionou ferramentas MCP para agentes operarem artifacts criativos e tokens.

Ferramentas adicionadas:

- `forge.creative.list`;
- `forge.creative.inspect`;
- `forge.creative.attach`;
- `forge.tokens.get`;
- `forge.tokens.set`.

Isso permite que agentes descubram e manipulem a base criativa, mas ainda não prova um produto criativo final.

## O Que Ainda Falta Para a 0.5

### 1. Live Collaboration

Status: **planejado**

Ainda precisa existir colaboração ao vivo real entre humano e IA.

Falta implementar/provar:

- presença;
- cursores;
- seleção;
- patch streams;
- comentários;
- resolução de conflito;
- rollback;
- histórico visual/auditável;
- colaboração em web UI, CLI e workflows assíncronos.

Esse é um dos maiores bloqueadores da 0.5.

### 2. Pesquisa Técnica Consolidada

Status: **planejado**

O Forge ainda precisa gerar um artifact de pesquisa comparando referências importantes.

Falta pesquisar e sintetizar:

- Penpot;
- Google Stitch;
- v0;
- Impeccable/AGUI/AG-UI-style protocols;
- Superpowers;
- Remotion como referência de workflow para vídeo longo;
- Figma capabilities;
- OBS/media composition;
- modelos de whiteboard colaborativo;
- modelos de design system e tokens.

O objetivo não é copiar essas ferramentas, mas transformar o que for útil em workflows e primitives do Forge.

### 3. Export/Demo Baseline

Status: **groundwork**

Ainda falta demonstrar o runtime criativo funcionando de ponta a ponta.

Faltam pelo menos dois demos:

1. Workflow de design/tokens/componentes:
   - criar artifact visual;
   - aplicar design system;
   - trocar tokens;
   - preservar edições humanas;
   - gerar saída renderizável ou exportável.

2. Workflow de documento/slide/whiteboard:
   - criar artifact estruturado;
   - permitir edição/patch;
   - exportar;
   - provar que continua editável.

Sem esses demos, a 0.5 não deve ser promovida.

### 4. Renderização e Edição Real

Status: **faltando**

Hoje existe IR e persistência, mas não uma experiência criativa completa.

Falta:

- renderer para telas;
- renderer para whiteboard;
- renderer para slides/documentos;
- preview;
- edição por humano;
- edição por IA preservando estrutura;
- validação visual;
- exportação confiável.

### 5. Token Resolution Engine

Status: **faltando**

Já existem tokens, mas falta o motor que faz eles realmente governarem o design.

Falta:

- resolução raw/semantic;
- temas;
- modes;
- overrides;
- precedência;
- preview de impacto;
- propagação global;
- regressão visual/estrutural.

### 6. Patch-by-Intent Executável

Status: **faltando**

Já existe schema, mas falta o executor real.

Exemplo esperado:

> “Troque o estilo primário do produto para mais institucional, preserve layout e conteúdo humano.”

O Forge deve transformar isso em patch estruturado, aplicar no IR, validar e mostrar impacto.

### 7. AGUI/AI-first Product Surface

Status: **faltando**

Já existe o goal de AGUI/AG-UI-style, mas ainda falta pesquisa + implementação concreta.

Falta:

- action registry real;
- UI tree inspecionável;
- eventos;
- permissões por ação;
- componentes preparados para agentes;
- interface humano+IA como parte do produto, não só como chat.

## Conclusão

A 0.5 está bem encaminhada, mas ainda não deve ser chamada de pronta.

O Forge já saiu de “ideia” para uma base técnica real:

- workflow runtime;
- schedule/loop/subflow;
- CLI interativo;
- decisões humanas;
- MCP;
- IR criativo;
- tokens;
- componentes.

Mas a parte que torna a 0.5 realmente especial ainda falta provar em execução:

- colaboração ao vivo;
- pesquisa técnica consolidada;
- demos criativos reais;
- renderização/export;
- token propagation;
- patch-by-intent executável.

Recomendação objetiva:

1. Criar o artifact de pesquisa 0.5.
2. Implementar token resolution engine.
3. Implementar primeiro demo design/tokens/componentes.
4. Implementar primeiro demo whiteboard/documento/slide.
5. Só então reavaliar promoção para `0.5`.

Até lá, o correto é dizer:

> Forge 0.4.x já contém groundwork forte para a 0.5, mas Forge 0.5 ainda não está pronta para promoção.
