# Requisitos normalizados — worktrees e sandboxes

Workflow: `wf_cf8a1698f266442981479112786f00b0`

| ID | Requisito mensurável | Evidência de aceite |
| --- | --- | --- |
| WT-01 | Descobrir, criar e registrar Git worktrees sem substituir a autoridade do Git. | Contrato `forge_worktree_contract` verde e relatórios versionados. |
| WT-02 | Persistir vínculos de workflow e task no store central, com precedência do vínculo de task. | Testes de binding, context e handoff. |
| WT-03 | Manter `.forge/worktree.toml` legível, relativo ao checkout e aprovado por SHA-256. | Drift bloqueia execução até `approve-config` e novo bind. |
| WT-04 | Permitir apenas paths contidos, sem symlink; paths protegidos vencem scopes modificáveis. | Nove testes do contrato de guardrails. |
| WT-05 | Criar predecessora objetiva para trabalho protegido, sem duplicar tarefa equivalente em retry. | DAG/revisão idempotentes e retorno automático da dependente a `pending`. |
| SB-01 | Planejar e executar sandboxes `process` com cwd, environment, timeout e output limitados. | Receipt `forge.worktree.sandbox_receipt.v1` e smoke `process-ok`. |
| SB-02 | Executar Bubblewrap com worktree read-only, sandbox/home/tmp graváveis e `network=deny`. | Smoke real `bubblewrap-ok`, sem conseguir criar `/workspace/forbidden`. |
| SB-03 | Persistir lifecycle preview/test com `start`, `status` e `stop`, exigindo autorização nas mutações. | Lifecycle `forge.worktree.sandbox_lifecycle.v1` e smoke CLI/MCP. |
| SB-04 | Impedir que processos `setsid` sobrevivam a timeout, stop ou queda do supervisor. | Regressões com pipes herdados, stdio redirecionado e SIGKILL do supervisor. |
| SB-05 | Serializar read-modify-write do lifecycle e manter evento/estado na mesma transação. | `BEGIN IMMEDIATE`, helpers raw sem transação aninhada e testes de lifecycle verdes. |
| SEC-01 | Bloquear segredos em argv e environment, inclusive nome neutro e token de alta entropia. | Guard decisions bloqueadas, sem execução e sem valor raw no SQLite/eventos. |
| SEC-02 | Redigir segredos em stdout, stderr e erros, preservando hashes/contagens. | `redaction_count > 0` e placeholders do vault nos receipts. |
| API-01 | Manter paridade CLI/MCP para sandbox plan/run/start/status/stop. | MCP contract e rejeição de `task_id` sem `workflow_id`. |
| DOC-01 | Documentar limites de isolamento, autorizações, reconciliação e replay. | README, definição técnica, guia dedicado, exemplo e skill modular sincronizados. |

Todos os requisitos acima possuem resultado binário observável e foram ligados a teste, receipt, smoke ou documentação versionada.
