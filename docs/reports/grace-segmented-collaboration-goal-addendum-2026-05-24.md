# GRACE - Addendum de Goal

## Colaboração Parcial Inteligente com Atribuição de Caminhão

**Workflow Forge:** `wf_710fe1d41a324dd4b22af04a65f53711`  
**Run Forge:** `run_65dd78cebf3748f4b8b02d75f2079bb3`  
**Status:** goal atualizado em tempo de execução  
**Tema:** Compartilhamento Segmentado de Capacidade

## Mudança no Goal

O GRACE não deve apenas identificar que uma carga pode ser dividida em trechos
colaborativos. O sistema também deve decidir **qual caminhão/veículo executa
cada trecho**.

Isso transforma o MVP de roteirização colaborativa simples em uma proposta de:

- colaboração parcial de rota;
- consolidação segmentada de cargas;
- otimização distribuída de capacidade;
- logística colaborativa multi-etapa;
- redução de emissão marginal por carga transportada.

## Exemplo Base do Pitch

Empresas:

- **Empresa A:** Arroio Grande -> Pelotas;
- **Empresa B:** Pelotas -> Rio Grande;
- **Empresa C:** precisa enviar carga de Arroio Grande -> Rio Grande.

Sem GRACE:

- um caminhão adicional faz Arroio Grande -> Rio Grande;
- há mais quilômetros totais;
- há mais emissão e consumo energético.

Com GRACE:

- o trecho Arroio Grande -> Pelotas pode ser feito pelo caminhão de A;
- ocorre uma transferência/intercâmbio em Pelotas;
- o trecho Pelotas -> Rio Grande pode ser feito pelo caminhão de B;
- o sistema registra qual veículo executa cada segmento e por quê.

## Regra de Decisão do Veículo

Para cada segmento candidato, o GRACE deve comparar os veículos disponíveis e
selecionar aquele com melhor score operacional e ambiental.

Critérios mínimos:

- sobreposição com a rota original do veículo;
- capacidade ociosa disponível;
- custo de desvio adicional;
- compatibilidade de janela de tempo;
- viabilidade do ponto de transferência;
- redução estimada de quilômetros equivalentes;
- redução estimada de combustível/energia;
- redução estimada de CO2;
- simplicidade operacional para um MVP demonstrável.

## Saída Esperada do MVP

Cada recomendação segmentada deve exibir:

- carga atendida;
- sequência de segmentos;
- veículo escolhido por segmento;
- justificativa legível;
- ponto de transferência;
- distância isolada;
- distância colaborativa equivalente;
- economia estimada;
- CO2 evitado;
- alerta quando a colaboração não for recomendada.

Exemplo de saída:

| Segmento | Veículo escolhido | Justificativa |
|---|---|---|
| Arroio Grande -> Pelotas | Caminhão A | já segue esse trajeto e possui capacidade ociosa |
| Pelotas -> Rio Grande | Caminhão B | minimiza quilometragem adicional após o ponto de transferência |

## Linguagem de Pitch

Usar:

- **Colaboração Parcial Inteligente**;
- **Compartilhamento Segmentado de Capacidade**;
- **consolidação segmentada de cargas**;
- **otimização colaborativa por trechos**.

Evitar:

- "troca de carga";
- "transbordo complexo";
- "operação logística pesada";
- "marketplace genérico de frete".

## Impacto na Implementação

O backlog de desenvolvimento deve incluir:

1. Representar uma demanda como origem, destino, volume, janela e restrições.
2. Representar veículos com rota planejada, capacidade livre e janela.
3. Quebrar uma demanda em segmentos possíveis com pontos de handoff.
4. Calcular rotas e desvios com OSRM.
5. Atribuir o melhor veículo para cada segmento.
6. Rejeitar recomendações quando o desvio ou risco operacional for alto.
7. Mostrar a recomendação em linguagem simples para o pitch.

## Critérios de Teste

O MVP deve ter pelo menos um teste de cenário:

- entrada: A faz Arroio Grande -> Pelotas;
- entrada: B faz Pelotas -> Rio Grande;
- entrada: C precisa Arroio Grande -> Rio Grande;
- esperado: segmento 1 atribuído ao veículo A;
- esperado: segmento 2 atribuído ao veículo B;
- esperado: a recomendação mostra redução de km equivalente e CO2;
- esperado: a justificativa evita linguagem de "troca de carga".

## Decisão

Essa feature deve ser tratada como diferencial principal do GRACE para o
hackathon. Ela aumenta a força ambiental e técnica da tese sem exigir que o MVP
implemente uma operação logística real em produção.
