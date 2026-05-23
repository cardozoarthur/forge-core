# Forge Core v0.4.4 Report - 2026-05-23

## Objetivo

Corrigir a saída de `forge self run --output json` para que artifacts de execução assíncrona continuem parseáveis por ferramentas.

## Implementado

- `run_validation` agora captura stdout/stderr dos comandos de validação.
- Em caso de sucesso, os logs do `cargo` não são misturados no stdout JSON do Forge.
- Em caso de falha, os logs capturados são enviados para stderr para diagnóstico.

## Impacto

O loop em `tmux` pode registrar `.forge/self-evolve-loop.jsonl` como JSON válido nas próximas execuções. Isso melhora acompanhamento automático, parsing de artifacts e envio posterior de relatórios.

## Validação esperada

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## Próximo passo

Adicionar um relatório de ciclo separado para stdout/stderr capturados, sem quebrar o contrato JSON do comando principal.
