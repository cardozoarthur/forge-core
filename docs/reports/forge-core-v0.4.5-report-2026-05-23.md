# Forge Core v0.4.5 Report - 2026-05-23

## Objetivo

Tornar a validação de `forge self run` uma evidência operacional persistente, não apenas um booleano no relatório de ciclo.

## Implementado

- Adicionado o artifact `self-evolution-cycle-NNN-validation.json` para cada ciclo de autoevolução.
- Adicionada a versão de schema `forge.self_evolution.validation.v1`.
- O relatório de ciclo agora inclui `validation_report_path` e `validation_report_sha256`.
- Cada comando obrigatório de validação registra status, exit code, duração, stdout e stderr capturados.
- Se um comando falhar, os comandos restantes são registrados como `skipped` e a promoção continua bloqueada.

## Impacto

Operadores e executores conseguem auditar exatamente qual evidência de validação sustentou ou bloqueou a promoção de um ciclo. O stdout principal do Forge continua seguro para JSON, enquanto os detalhes completos ficam persistidos em artifact versionado.

## Validação

Comandos obrigatórios executados neste ciclo:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Resultado: todos passaram.

## Instalação local

- `cargo install --path . --force` foi tentado primeiro, mas o sandbox bloqueou escrita em `/home/arthur/.cargo/.crates.toml` com `Read-only file system`.
- A instalação local gravável do checkout foi atualizada com:

```bash
CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline
```

- O binário local em `.forge/local-install/bin/forge` passou a reportar `forge 0.4.5`.
- O `forge` global no PATH do usuário ainda reporta `forge 0.4.4` dentro desta sessão.

## Próximo passo

Adicionar um resumo compacto dos artifacts de validação em `forge request status`, para que chamadas assíncronas consigam enxergar a última evidência sem precisar listar artifacts manualmente.
