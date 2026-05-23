# Forge Core v0.4.1 Report - 2026-05-23

## Objetivo

Criar um contrato mínimo e auditável para prompts de auto-evolução, tratando cada prompt enviado a Codex/OpenCode como um pacote versionado em vez de texto solto.

## Implementado

- `forge self run` agora gera prompts com `Prompt packet version: forge.self_evolution.prompt.v1`.
- Cada `SelfCycleReport` inclui:
  - `prompt_packet_version`;
  - `prompt_sha256`.
- O SHA-256 é calculado sobre o prompt Markdown escrito em `self-evolution-cycle-*-prompt.md`.
- Os comandos obrigatórios de validação ficam listados dentro do prompt packet.

## Contrato operacional

O relatório do ciclo permite responder:

- qual versão do contrato de prompt o executor recebeu;
- qual arquivo de prompt foi persistido;
- se o prompt auditado localmente corresponde ao hash persistido.

Isso prepara o caminho para adapters reais, leases e comparação de regressões de prompt sem tornar Codex/OpenCode a fonte de verdade do workflow.

## Validação esperada

O contrato é coberto pelo teste:

```bash
cargo test self_run_prompt_packet_is_versioned_and_checksummed_for_executor_replay
```

A validação completa desta versão continua exigindo:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## Próximo passo

Adicionar leases de task para impedir que dois executores adquiram o mesmo trabalho assíncrono ao mesmo tempo.
