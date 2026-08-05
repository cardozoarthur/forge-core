# Migração de Forge para Foundry

Este documento é o contrato canônico de rebranding do produto. A partir da
série `0.6.x`, o nome do produto é **Foundry Core** e a interface canônica é
`foundry`. O nome Forge identifica somente a geração anterior e as entradas de
compatibilidade descritas abaixo.

## Matriz de nomes

| Superfície | Legado (Forge) | Canônico (Foundry) |
| --- | --- | --- |
| Produto | Forge Core | Foundry Core |
| Repositório | `cardozoarthur/forge-core` | `cardozoarthur/foundry-core` |
| CLI | `forge` | `foundry` |
| Crate/pacote Rust | `forge-core` / `forge_core` | `foundry-core` / `foundry_core` |
| Tools e schemas | `forge.*` | `foundry.*` |
| Variáveis de ambiente | `FORGE_*` | `FOUNDRY_*` |
| Estado de projeto | `.forge/` | `.foundry/` |
| Store local padrão | `.forge/forge.sqlite` | `.foundry/foundry.sqlite` |
| Skills | `forge-core*` | `foundry-core*` |
| SDK Python | `forge-sdk` / `forge_sdk` / `Forge*` | `foundry-sdk` / `foundry_sdk` / `Foundry*` |
| SDK TypeScript | `@forge/core-sdk` / `Forge*` | `@foundry/core-sdk` / `Foundry*` |
| SDK Go | `cardozoarthur/forge-core/sdk/go` / `package forge` | `cardozoarthur/foundry-core/sdk/go` / `package foundry` |
| SDK Rust | `forge-sdk-rust` | `foundry-sdk-rust` |
| Serviços e diretórios de host | `forge-*`, `/etc/forge`, `/var/lib/forge` | `foundry-*`, `/etc/foundry`, `/var/lib/foundry` |

## Janela de compatibilidade `0.6.x`

A série `0.6.x` oferece um ciclo de compatibilidade para permitir migração
controlada. Compatibilidade não significa duas marcas ativas nem dois estados
gravados em paralelo:

- `foundry` é o comando documentado e emitido por todos os novos planos,
  receipts, mensagens e exemplos;
- um shim `forge` pode encaminhar os mesmos argumentos para `foundry`, preservar
  código de saída e sinais, e emitir um aviso de depreciação sem incluir
  segredos ou payloads;
- nomes `FORGE_*` são aceitos apenas como fallback quando a variável
  `FOUNDRY_*` correspondente não está presente;
- quando as duas formas existem, `FOUNDRY_*` sempre vence. A presença da
  variável nova, inclusive vazia, impede fallback silencioso para a antiga;
- tools, schemas, Addon IDs e inputs `forge.*` existentes podem ser aceitos por
  aliases de entrada. Novos outputs usam `foundry.*` por padrão;
- registros persistidos com `forge.*`, `forge_cli`, `forge_core_builtin` ou
  outros owners antigos continuam legíveis e não são regravados apenas para
  trocar a marca;
- não há dual-write entre `.forge` e `.foundry`, nem entre namespaces de
  contratos. Uma operação grava somente no store selecionado;
- aliases devem aparecer como `legacy`, `deprecated` ou `compatibility` na
  introspecção. Eles não podem voltar a ser sugeridos como caminho principal.

A remoção desses aliases não ocorre durante `0.6.x`. O primeiro corte elegível
é a próxima série incompatível, nunca antes de release notes com instruções de
migração e telemetria local sem segredos indicando que os aliases deixaram de
ser necessários.

## Stores e dados históricos

Foundry não move, copia, renomeia nem apaga automaticamente
`.forge/forge.sqlite`, diretórios de host ou backups antigos. Isso evita criar
um store vazio por engano, copiar um banco WAL de forma inconsistente ou tornar
o rollback impossível.

Tabelas e campos internos historicamente nomeados com Forge também não são
renomeados automaticamente. Em especial, dados assinados, hashes, receipts,
schemas registrados e a tabela histórica `forge_missions` preservam a
identidade original. Esses nomes são formato de armazenamento legado, não a
marca pública do produto.

Durante a janela de compatibilidade, um operador pode continuar abrindo o store
antigo explicitamente:

```bash
foundry --store .forge/forge.sqlite store check --output json
```

Não alterne gravações entre o store antigo e uma cópia nova. Escolha um único
store ativo por vez.

## Migração explícita de um projeto local

1. Atualize para uma release Foundry verificada e confirme
   `foundry --version`.
2. Pause schedulers, supervisores e qualquer outro writer do store antigo.
3. Verifique o store antigo explicitamente:

   ```bash
   foundry --store .forge/forge.sqlite store check --output json
   ```

4. Crie `.foundry/` e use o backup SQLite consistente do próprio runtime; não
   copie um banco WAL ativo com `cp`:

   ```bash
   mkdir -p .foundry
   foundry --store .forge/forge.sqlite store backup \
     --destination .foundry/foundry.sqlite \
     --output json
   foundry --store .foundry/foundry.sqlite store check --output json
   ```

5. Migre apenas configurações humanas necessárias para `.foundry/`, revisando
   paths, executáveis e permissões. Não copie chaves, sockets, PIDs ou caches
   sem entender seu ciclo de vida.
6. Inicie Foundry apontando exclusivamente para
   `.foundry/foundry.sqlite` e valide workflows, artifacts, schedules, leases e
   health checks.
7. Mantenha `.forge/` sem writers durante a janela de rollback. Só arquive ou
   remova o legado após a validação operacional e conforme a política local de
   retenção.

## Migração de automações e integrações

Para cada script, CI, Addon, MCP client ou serviço:

1. troque o binário para `foundry`;
2. troque `FORGE_*` por `FOUNDRY_*` e remova a variável antiga após validar a
   precedência;
3. troque tools e schemas gerados para `foundry.*`;
4. troque `.forge/` por `.foundry/` somente depois de escolher o store ativo;
5. troque skills para `foundry-core*` e SDKs para seus pacotes Foundry;
6. execute o fluxo real, confirme receipts novos com `foundry.*` e procure
   avisos de compatibilidade;
7. remova o alias legado daquela integração antes do fim da janela `0.6.x`.

## Rollback

O rollback troca executável, configuração e store como um conjunto. Pare os
writers Foundry antes de reativar uma instalação antiga. Uma versão Forge pode
não reconhecer versões futuras do schema SQLite; portanto, use o store legado
preservado ou um backup validado compatível, nunca a cópia que continuou
recebendo gravações Foundry.

Relatórios, releases, SBOMs, attestations, assinaturas e URLs da era Forge são
evidência histórica imutável. Eles não devem ser reemitidos ou reescritos como
se tivessem sido produzidos por Foundry.
