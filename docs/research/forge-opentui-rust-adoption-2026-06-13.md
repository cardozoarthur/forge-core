# Forge TUI: adoção OpenTUI em Rust

Data: 2026-06-13

## Fontes verificadas

- <https://github.com/anomalyco/opentui> em `/tmp/opentui`, commit `90db181`.
- <https://github.com/msmps/create-tui> em `/tmp/create-tui`, commit `6f12ee9`.
- <https://github.com/msmps/awesome-opentui> em `/tmp/awesome-opentui`, commit `73e59da`.
- <https://github.com/msmps/opentui-ui> em `/tmp/opentui-ui`, commit `8396024`.

## Decisão

O `forge` deve continuar abrindo uma TUI nativa apenas com `forge` no terminal. A lógica do produto, do orquestrador, dos workflows, dos agentes, dos subagentes, dos handoffs, dos approvals e dos custos fica em Rust. O renderer atual em `crossterm` fica como fallback obrigatório para servidores e ambientes sem Bun/Zig.

O alvo visual deve ser um backend OpenTUI por adaptador, não uma reescrita imediata e integral do OpenTUI em Rust. O OpenTUI já concentra o trabalho difícil de terminal bonito: renderer nativo em Zig, buffer otimizado, layout flex, hit grid de mouse, texto Unicode, editor/text buffer, render loop, split footer, console/remote feed e estatísticas de frame.

## Portar para Rust

- Árvore de estado do Forge: workflows ativos, runs, eventos, schedules, Addons/capabilities, custos, handoffs e approvals.
- Roteamento do orquestrador: entrada normal conversa com o orquestrador; `/...` configura; `!` executa shell auditado.
- Contratos de workflow/agente/subagente/node-agent.
- Modelo de componentes do Forge independente do renderer: painel, prompt, tab, badge, lista, timeline, modal, toast e tabela.
- Metadados de componentes no estilo `opentui-ui`: slots, estados e variantes.
- Estados comuns: focused, selected, disabled, loading, errored, pending approval e running.
- Testes de contrato JSON, smoke end-to-end e captura pseudo-TTY.

## Usar por ponte ou manter externo

- Renderer OpenTUI Zig/C ABI e buffer otimizado.
- Yoga/flex layout e reconciliação de árvore visual.
- Grapheme width, Unicode, text buffer, edit buffer, markdown/diff/code renderables.
- Mouse hit grid, split footer, scrollback, remote feed e frame stats.
- Templates `create-tui` Core/React/Solid para protótipos gerados de superfícies externas.
- Componentes `opentui-ui` como referência de API, não dependência obrigatória do binário Rust.

## Como `awesome-opentui` ajuda

`awesome-opentui` serve como mapa do ecossistema. Para o Forge, os itens mais úteis são:

- OpenCode como referência de experiência de agente no terminal.
- `anscribe` e `pilotty` como direção para inspeção/teste automatizado de TUI por agentes.
- Ferramentas como `tuiboard`, `critique`, `hunk`, `restman` e `t-req` como exemplos de painéis operacionais, diff/review, HTTP workspaces e gestão de trabalho no terminal.

## Como `opentui-ui` ajuda

`opentui-ui` mostra a camada que o Forge precisa ter antes de trocar o renderer:

- Componentes com slots nomeados.
- State selectors para aparência dependente de foco, seleção, erro e loading.
- Variantes tipadas para intenção, tamanho e densidade.
- Dialog manager com confirm, alert, prompt e choice, aplicável a handoffs/approvals.
- Toasts com success, error, warning, info, loading, atualização por id e duração controlada.

## Próximo passo técnico

Criar um `forge_tui_renderer` com trait de backend:

```rust
trait ForgeTuiRenderer {
    fn render(&mut self, state: &ForgeTuiState) -> anyhow::Result<()>;
    fn poll_event(&mut self) -> anyhow::Result<ForgeTuiEvent>;
    fn shutdown(&mut self) -> anyhow::Result<()>;
}
```

Backends iniciais:

- `RustCrosstermFallback`: backend atual, obrigatório e testado.
- `OpenTuiNativeBridge`: backend futuro, feature flag, usando OpenTUI como renderer nativo.
- `GeneratedOpenTuiTemplate`: opção para gerar protótipos Addon via `create-tui`, sem virar dependência do core.

## Critério de aceite

- `forge` abre a TUI fullscreen sem subcomando.
- `forge tui --output json` expõe renderer strategy e fontes OpenTUI/create-tui/awesome-opentui/opentui-ui.
- Smoke end-to-end continua passando sem Bun/Zig.
- O backend OpenTUI pode ser adicionado depois sem mudar os contratos do orquestrador.
