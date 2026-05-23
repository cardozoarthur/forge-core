# Forge Core v0.3.0 Report - 2026-05-23

## Objetivo

Adicionar a base para execução assíncrona e mutação de workflow em tempo real.

## Implementado

- Docker, Kubernetes e Knative são detectados como run substrates.
- Run substrates têm política própria, separada dos executores cognitivos.
- `forge sync runtimes` detecta runtime local e pede/autentica permissão humana.
- `forge sync all` sincroniza executores e runtimes juntos.
- `forge runtimes` mostra a política persistida.
- Se Docker e Kubernetes estão disponíveis, mas Knative não está, Forge sugere instalação do Knative, sempre exigindo autorização humana.
- `forge runtime guard` aplica a regra de ownership:
  - recursos criados pelo Forge podem ser atualizados/deletados;
  - recursos externos/preexistentes exigem autorização explícita.
- Tasks que pedem Docker/Kubernetes/Knative ou execução assíncrona recebem `async_policy`.
- `forge workflow update-goal` altera o goal em runtime com origem rastreada.
- `forge workflow attach-artifact` copia artifact para o storage do workflow e registra origem.
- `status` agora expõe revisões do workflow.

## Estado na máquina

Sync real executado com `forge 0.3.0`:

- Codex: instalado, configurado e autorizado.
- OpenCode: instalado, configurado e autorizado.
- `opencode_codex_bridge`: habilitado.
- Docker: instalado, configurado e autorizado.
- Kubernetes: `kubectl` detectado, mas sem kubeconfig no `$HOME`; indisponível para Forge.
- Knative: não detectado; indisponível para Forge.

Como Kubernetes não está configurado nesta máquina, o Forge não tentou usar Knative nem sugerir instalação real no cluster atual. Isso respeita a regra de não modificar infraestrutura fora de contexto.

## Smoke executado

Workflow criado:

- `wf_8c0ef9abc31341ee99b983d60fd3ed3f`

Mutação em runtime:

- Goal atualizado com origem `codex`.
- Artifact anexado com origem `opencode`.
- Status mostrou 2 revisões persistidas.
- Tasks do workflow receberam `async_policy.mode = async`.

Guard de runtime:

- `service/external-api` com owner `external`: bloqueado, exige autorização humana.
- `service/forge-node` com owner `forge`: permitido.

## Interface humana

Codex CLI e OpenCode CLI passam a ser tratados como interfaces humanas do Forge, além de futuros executores:

- podem atualizar goals;
- podem anexar artifacts;
- podem disparar sync;
- podem pedir contexto bounded;
- não devem bypassar o estado persistente do Forge.

## Segurança operacional

Forge não pode modificar infraestrutura do usuário fora do escopo dele sem permissão.

Regra:

- Forge-owned: permitido.
- Externo/preexistente: bloqueado por padrão.
- Externo com autorização explícita: permitido e rastreado.

## Próxima versão

v0.4.0 deve implementar adapters reais:

- leases;
- execução longa via OpenCode/Codex;
- Knative node adapter com labels de ownership;
- invalidação de contexto downstream quando goal/artifact muda;
- scheduler daemon para workflows assíncronos reais.
