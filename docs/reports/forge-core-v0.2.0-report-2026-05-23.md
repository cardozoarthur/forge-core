# Forge Core v0.2.0 Report - 2026-05-23

## Objetivo

Transformar o Forge em um runtime mais orientado a execução real: ele precisa detectar executores locais, pedir/autenticar permissão humana, persistir a política, tratar tasks como goals definitivos e evoluir estruturalmente com changelog forte por versão.

## O que foi implementado

- `forge sync executors` detecta CLIs conhecidos na máquina.
- `forge executors` mostra a política persistida.
- `forge skill install` roda sync de executores durante a instalação.
- Codex e OpenCode podem ser autorizados explicitamente com `--allow codex --allow opencode`.
- Quando Codex e OpenCode estão autorizados, o Forge registra `opencode_codex_bridge`.
- Cada task agora possui `goal` próprio.
- Cada task possui work item com backlog state, prioridade, owner, subtasks, impedimentos, critérios de aceite e validação de goal.
- Cada subtask possui goal e definition of done.
- `forge validate` bloqueia promoção quando o goal não está definitivamente pronto e retorna `rework_tasks`.
- `forge run --simulate` marca tasks/subtasks como concluídas e goals como definitivamente prontos.
- `forge improve --target-version` gera experimento controlado e changelog Markdown.

## Estratégia de evolução

O Forge não deve melhorar apenas por prompt tuning. A evolução agora fica dividida em domínios:

- estrutura de tasks;
- sistema de prompts;
- processo runtime;
- governança de validação;
- política de executores.

Isso permite que ele evolua como um sistema operacional de trabalho: backlog, pendências, impedimentos, subtasks, validação, rework e changelog.

## Estado operacional

O Forge v0.2.0 foi instalado localmente e sincronizado nesta máquina.

Executores:

- Codex: instalado, configurado e autorizado.
- OpenCode: instalado, configurado e autorizado.
- `opencode_codex_bridge`: habilitado.
- Gemini, Claude e Ollama: não detectados/configurados nesta máquina.

O caminho implementado ainda não executa Codex/OpenCode automaticamente como adapters reais de longa duração. Ele prepara a camada obrigatória antes disso:

- detecção;
- autorização humana;
- persistência;
- ponte OpenCode/Codex;
- contrato goal-oriented;
- validação com rework.

O próximo passo técnico é implementar leases/adapters reais para que o Forge consiga chamar Codex/OpenCode com task packets bounded.

## Execução do próprio Forge

Foi criado um workflow de evolução estrutural:

- Workflow: `wf_fabdfa4b754646bcb7bb4dac3fe5559c`
- Primeiro `forge validate`: bloqueado por `task_status` e `goal_readiness`, com `rework_tasks` explícitos.
- Depois de `forge run --simulate`: `forge validate` passou com `promotable=true`, sem `failed_rules` e sem `rework_tasks`.
- `forge improve --target-version 0.3.0` gerou artifacts de evolução.

Artifacts:

- `artifacts/wf_fabdfa4b754646bcb7bb4dac3fe5559c/improvement-20260523T203023Z.json`
- `artifacts/wf_fabdfa4b754646bcb7bb4dac3fe5559c/changelog-0.3.0.md`

## Validação executada

Comandos executados com sucesso:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
cargo install --path . --force
forge --version
forge --store .forge/forge.sqlite sync executors --home "$HOME" --allow codex --allow opencode --no-prompt --output json
```

Resultado:

- `forge 0.2.0`
- 15 testes passaram.
- Build release concluído.
- Política local de executores persistida com Codex/OpenCode autorizados.
- Skill instalada para Codex/OpenCode com sync embutido.

## Próximo passo recomendado

A v0.3.0 começou a transformar esse próximo passo em contrato runtime:

- detecção de Docker/Kubernetes/Knative como substratos assíncronos;
- guard de recursos para impedir mutação de recursos externos sem autorização;
- mutação de goals/artifacts em runtime com origem rastreável;
- Codex/OpenCode como interfaces humanas para atualizar o estado do Forge.

Ainda falta implementar adapters reais de execução prolongada:

- task leases;
- task packet schema;
- execução bounded via OpenCode;
- execução bounded via Codex;
- execução real via Docker/Kubernetes/Knative;
- captura de trace/custo;
- validação do retorno do executor;
- rework automático quando o goal não estiver definitivamente pronto.
