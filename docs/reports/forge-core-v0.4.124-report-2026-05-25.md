# Forge Core v0.4.124 — Self-Evolution Gate Hotfix

## Resumo

O loop de autoevolução estava preservando os goals, mas o gate de decisão estava tratando a fase nova do Forge 0.5 como trabalho caro demais ou como já encerrado por uma regra terminal antiga.

Esta versão corrige isso: goals explícitos de continuação para Forge 0.5, integração com agentes, MCP/skills, CLI/TUI interativo, creative runtime, colaboração humano+IA, design tokens e relatórios por Telegram agora mantêm o ciclo em `run_cycle`.

## O Que Foi Corrigido

- `terminal_goal_contract_satisfied` não encerra mais o ciclo quando o goal contém uma continuação humana explícita, como `do not stop`, `continue until`, `Forge 0.5`, `creative runtime`, `interactive Forge CLI`, `live human+AI collaboration` ou `version-boundary`.
- `expected_value_score` passou a reconhecer termos estratégicos da fase 0.5: MCP, skills, agent integration, creative runtime, slash commands, TUI, direct-chat routing, human decisions/forms, live collaboration, whiteboards, design systems/tokens, componentization, creative artifacts, milestone manifest e Telegram.
- Foi adicionado teste de contrato garantindo que o objetivo 0.5 não seja parado nem rejeitado.

## Validação Executada

- `cargo fmt --check`
- `cargo test self_run_keeps_running_for_explicit_forge_05_continuation_goal --test forge_cli_contract`
- `cargo test self_run_stops_when_terminal_final_goal_contract_is_satisfied --test forge_cli_contract`
- `cargo test self_run_rejects_low_value_bloat_cycle_in_lean_mode --test forge_cli_contract`

## Estado Esperado Agora

O workflow de autoevolução deve continuar trabalhando na fase Forge 0.5 até validar e reportar:

- superfícies MCP/skills/agentes;
- Forge CLI/TUI interativo sem subcomando;
- slash commands e roteamento de conversa direta;
- creative runtime para design, documentos, slides, vídeo e whiteboards;
- design system e design tokens;
- colaboração humano+IA;
- evidência lean de que a complexidade adicionada aumenta throughput útil;
- manifest de milestone 0.5 separando pronto, parcial e pendente.

## Observação

Esse hotfix não conclui a versão 0.5. Ele remove o bloqueio que fazia o Forge parar antes de trabalhar nela.
