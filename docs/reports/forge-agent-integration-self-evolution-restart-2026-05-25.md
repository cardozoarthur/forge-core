# Forge Core — Reinício da Autoevolução para Integração com Agentes

Data: 2026-05-25  
Origem: Codex  
Run: `run_700c2576127f4f96a5bb4a4f3ac97d7d`  
Workflow principal: `wf_1d8a0cf63ed147d7b4e140ac9d75cf9c`

## Objetivo desta fase

Reativar o ciclo de autoevolução do Forge Core com foco na camada de integração com agentes. A prioridade deixou de ser ampliar governança interna e passou a ser tornar o Forge um runtime que agentes conseguem chamar, observar e reutilizar de forma assíncrona.

## Goal terminal

O Forge deve parar esta fase somente quando existir:

- uma superfície MCP validada para invocação de workflows;
- skills utilizáveis por Codex/OpenCode para tarefas repetitivas;
- um fluxo de handoff em que o agente inicia uma execução, recebe um `run_id` rapidamente e inspeciona o progresso depois;
- validações e artifacts suficientes para provar que a integração funciona.

## Prioridades operacionais

1. Criar interface MCP para agentes listarem workflows, inspecionarem grafos, iniciarem runs, retomarem runs, alterarem goals/artifacts com revision tracking, pedirem contexto limitado e recuperarem validações/artifacts.
2. Gerar e manter skills Codex/OpenCode para iniciar runs assíncronos, inspecionar workflows, anexar artifacts, criar/reusar subflows e usar Forge como executor limitado.
3. Melhorar contratos de handoff entre agentes e Forge com `run_id`, `workflow_id`, `policy`, `allowed_context`, `validation_rules` e status consultável.
4. Preservar o modo lean: só promover mudanças que reduzam trabalho repetitivo, aumentem reuso, melhorem integração com agentes ou diminuam custo/contexto operacional.
5. Produzir docs, testes, changelog e artifacts de relatório a cada incremento.

## Estado inicial validado

- `forge sync all` encontrou `codex` e `opencode` instalados, configurados e autorizados.
- A ponte `opencode_codex_bridge` está habilitada.
- Docker está disponível como runtime assíncrono autorizado.
- Kubernetes está instalado, mas não configurado/autorizado para mutação.
- Knative não está disponível.
- O workflow principal iniciou com 11 tarefas pendentes.
- A primeira tarefa está pronta para handoff; as demais estão bloqueadas por dependências, como esperado.

## Restrições

- Não modificar Docker, Kubernetes ou Knative fora de recursos próprios do Forge sem autorização explícita.
- Não promover autoatualizações sem validação, benchmark e evidência.
- Não aumentar complexidade de runtime se ela não reduzir trabalho repetitivo, custo de contexto ou esforço operacional real.

## Próxima execução

O ciclo deve rodar com Codex e OpenCode como executores autorizados, intervalo de 5 minutos entre ciclos e parada temporal em 2026-05-26 às 10:00 no horário de São Paulo, salvo se o goal terminal for atingido antes.
