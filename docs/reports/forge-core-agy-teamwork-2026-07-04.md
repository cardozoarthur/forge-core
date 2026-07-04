# Forge Core agy/teamwork validation - 2026-07-04

## Resultado

Forge Core agora trata `agy` como executor funcional de primeira classe ao lado de Codex. O alias legado `antigravity` permanece apenas para compatibilidade, mas a rota operacional nova usa o comando real `agy`, com entrypoint `agy --print <prompt>`.

Gemini foi mantido como caminho legado/inválido para novas rotas de teamwork e self-evolution. Ele ainda pode aparecer como executor detectado no ambiente, mas a política nova o marca como `skipped_legacy_invalidated` nas rotas modernas.

## O que foi implementado

- `src/executor.rs`
  - Adiciona executor real `agy` com comando `agy`.
  - Detecta configuração em `~/.gemini/antigravity-cli`.
  - Expõe brain router com entrypoints `agy` e `agy --print <prompt>`.
  - Habilita integração `agy_codex_bridge` quando Codex e `agy` estão instalados, configurados e autorizados.
  - Reordena política de quota para `codex -> agy -> opencode`; Gemini fica em tier 99 como legado invalidado.

- `src/self_evolve.rs`
  - Define executores default `codex -> agy -> opencode`.
  - Executa fallback real via `agy --print`.
  - Ordena seleção por cadeia solicitada primeiro e capacidade depois.
  - Registra repair goals para `agy`, não para Gemini.

- `src/teamwork.rs`
  - Adiciona estratégia `forge.teamwork.strategy.v1` inspirada no padrão observado do `/teamwork-preview` do Antigravity: revisão de prompt/goal, onda de execução paralela e auditoria/promoção.
  - Define roster com 3 agentes: Orchestrator, Worker, Auditor.
  - Seleciona `agy` como Worker para UI/visual/dashboard, sem depender do alias `antigravity`.

- `src/graph.rs`, `src/interactive.rs`, `src/skill.rs`, skills e docs
  - Atualizam allowed brains para `codex`, `agy`, `opencode`, `claude`.
  - Removem Gemini das rotas modernas.
  - Atualizam exemplos e compatibilidade para orientar autorização do executor `agy`.

## Evidência sobre Antigravity teamwork-preview

O comando público `agy teamwork-preview --help` não existe no CLI local; o comportamento observado é um padrão interno de slash-command `/teamwork-preview`. A adaptação no Forge não copia um subcomando inexistente: ela modela o fluxo observado como uma estratégia Forge com estado, gates e artefatos rastreáveis.

## Validação Forge Core

Comandos obrigatórios executados:

- `cargo fmt --check`: passou.
- `cargo clippy --all-targets --all-features -- -D warnings`: passou.
- `cargo test`: passou com 679 testes em 9 suítes.
- `cargo build --release`: passou.

Smokes:

- `forge plan --goal "Create a delivery platform" --output json`: passou; `allowed_brains` em nós AI/mixed: `agy`, `claude`, `codex`, `opencode`.
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-agy2`: passou; 18 arquivos instalados.
- `forge sync executors --allow codex --allow agy --no-prompt --home "$HOME" --output json`: passou; `usable=["agy","codex"]`, selected brain `codex`, bridge `agy_codex_bridge.enabled=true`.
- `forge teamwork --goal "Build a dashboard UI" --output json`: passou; Worker=`agy`, primary brains=`codex, agy, opencode`, legacy invalidated=`gemini`.

Validação direta dos CLIs:

- `codex --version`: `codex-cli 0.142.4`.
- `agy --version`: `1.0.16`.
- `agy --print "Return exactly: forge-agy-ok"`: respondeu `forge-agy-ok`.

## Forge Desktop

Verificação atual em `/home/arthur/projects/forge-desktop`:

- `npm run build`: passou (`tsc && vite build`).
- `./node_modules/.bin/electron --version`: `v31.7.7`.
- `/home/arthur/projects/forge-core/target/release/forge list --output json`: passou a partir do diretório do desktop.
- GUI não foi lançada porque `DISPLAY` está vazio neste ambiente.

## Observações

- O smoke `forge validate` no workflow de plano retornou `blocked` porque o workflow era apenas planejado e todas as tarefas estavam pendentes. Isso é esperado para um smoke de planejamento e não indica falha de build.
- O sync de executor confirma que Codex e `agy` estão instalados, configurados, autorizados e prontos para uso não interativo no store de smoke.
