# GreenRoute Energia Compartilhada

## Ideia Final para o Ideathon Energia para Todos

**Nome recomendado:** GreenRoute Energia Compartilhada  
**Base da ideia original:** GreenRoute AI  
**Decisão de viabilidade:** `viable_with_reframe`  
**Workflow Forge:** `wf_710fe1d41a324dd4b22af04a65f53711`  
**Run Forge:** `run_65dd78cebf3748f4b8b02d75f2079bb3`

## Resumo Executivo

A GreenRoute Energia Compartilhada é uma plataforma web que ajuda pequenas
empresas, cooperativas e operações locais a reduzir consumo de combustível,
desperdício energético e emissões de CO2 por meio de logística colaborativa.

O MVP usa OSM e OSRM para simular rotas, comparar trajetos isolados contra
trajetos colaborativos e recomendar compartilhamento de capacidade ociosa em
veículos. A proposta transforma o problema de veículos vazios ou subutilizados em
uma solução de eficiência energética e inclusão digital para pequenos negócios.

## Encaixe no Regulamento

O regulamento valoriza tecnologia e inclusão digital aplicadas ao setor
energético e ao consumo sustentável. A ideia original de logística colaborativa é
promissora, mas precisa evitar parecer apenas um marketplace de frete.

O enquadramento correto é:

- **redução do consumo energético:** menos quilômetros redundantes e menos
  veículos circulando com baixa ocupação;
- **redução da pegada de carbono:** estimativa de CO2 evitado por consolidação
  de rotas;
- **eficiência energética em comércios e pequenas operações:** uso mais eficiente
  da energia já gasta no transporte;
- **mudança de hábitos de consumo:** incentivo a planejamento colaborativo em
  vez de transporte isolado;
- **inclusão digital:** acesso a otimização logística que normalmente só grandes
  empresas conseguem pagar.

## Matriz de Avaliação

| Critério | Peso | Avaliação da Ideia |
|---|---:|---|
| Aderência ao tema | 10% | Boa, desde que o pitch foque energia, emissões e consumo sustentável |
| Inovação e criatividade | 20% | Forte: combina colaboração logística, IA e sustentabilidade |
| Viabilidade técnica | 20% | Forte para MVP: OSM, OSRM, dashboard e heurística de matching são viáveis |
| Impacto social e ambiental | 20% | Forte: reduz custo operacional e emissões em pequenas operações |
| Modelo de negócio/funding | 10% | Moderado a forte: SaaS leve, taxa por otimização e relatórios ESG |
| Qualidade do pitch | 20% | Forte se a narrativa mostrar antes/depois, economia e CO2 evitado |

**Score estimado após reenquadramento:** 86/100.

## Problema

Pequenas empresas, cooperativas e comércios locais muitas vezes realizam
entregas, coletas e deslocamentos com veículos parcialmente vazios, rotas
repetidas e baixa coordenação entre operações compatíveis.

Isso gera:

- consumo desnecessário de combustível e energia;
- aumento de emissões;
- custos logísticos maiores;
- desperdício de capacidade de transporte;
- dificuldade de acesso a ferramentas de otimização;
- menor competitividade para pequenos negócios.

## Solução

A GreenRoute Energia Compartilhada coordena pedidos logísticos compatíveis e
mostra quando uma operação pode compartilhar capacidade de veículo ou combinar
rotas com outra operação.

O sistema calcula:

- rota individual de cada operação;
- rota colaborativa sugerida;
- quilômetros economizados;
- estimativa de combustível economizado;
- estimativa de CO2 evitado;
- aproveitamento de capacidade;
- economia financeira aproximada;
- score de compatibilidade entre cargas e rotas.

## MVP Recomendado

O MVP deve ser demonstrável sem integrações produtivas complexas.

Escopo essencial:

- cadastro manual de demandas de transporte com origem, destino, janela de tempo
  e capacidade necessária;
- cadastro de veículos ou rotas disponíveis com capacidade ociosa;
- mapa com rotas via OSM/OSRM;
- comparação entre cenário isolado e cenário colaborativo;
- recomendação de pareamento por heurística simples;
- painel de economia estimada;
- painel de CO2 evitado;
- página de pitch com problema, solução, impacto e modelo de negócio.

Fora do MVP:

- pagamento real;
- marketplace público;
- precificação dinâmica avançada;
- rastreamento em tempo real;
- contrato jurídico entre empresas;
- integração com ERPs ou telemetria veicular.

## Abordagem Técnica

Arquitetura sugerida para o MVP:

- frontend web com mapa e dashboard;
- API simples para cadastrar rotas, veículos e demandas;
- OSRM para cálculo de distância e tempo;
- OSM como base cartográfica;
- heurística de matching por proximidade de origem/destino, janela de tempo,
  capacidade e desvio máximo permitido;
- cálculo de emissão baseado em distância, tipo de veículo e fator médio de
  emissão;
- dados de demonstração para simular pequenas empresas locais.

## Heurística Inicial

Uma recomendação é aceita quando:

- origem ou destino ficam dentro de um raio configurável;
- janela de tempo é compatível;
- capacidade ociosa comporta a carga;
- desvio da rota original fica abaixo de um limite;
- economia estimada supera o custo operacional adicional;
- redução estimada de emissão é positiva.

## Plano de Fábrica MVP

1. Entendimento do problema e regulamento.
2. Validação de viabilidade da ideia.
3. Reenquadramento para energia, inclusão digital e sustentabilidade.
4. Criação do artifact de ideia final.
5. Desenho do fluxo principal do usuário.
6. Implementação do cadastro de demandas, veículos e rotas.
7. Integração OSM/OSRM para cálculo de rotas.
8. Implementação do matching colaborativo.
9. Implementação do painel de economia e CO2.
10. Preparação de dados demo e roteiro de apresentação.
11. Testes de fluxo principal e cálculos.
12. Pitch de até 5 minutos e até 10 slides.
13. Melhorias recorrentes até o prazo com buffer.

## Prazo e Buffer

Prazo oficial de entrega: **31/05/2026 às 23h59**.  
Buffer recomendado configurável: **36 horas**.  
Parada operacional recomendada para organização da equipe:
**30/05/2026 às 11h59**.

Esse buffer deve ser usado para:

- congelar escopo;
- revisar pitch;
- gravar vídeo;
- conferir slides;
- testar demo;
- organizar submissão.

## Roteiro de Pitch

Estrutura para 5 minutos:

1. O problema: veículos vazios desperdiçam energia, dinheiro e geram emissões.
2. A solução: coordenação colaborativa de rotas e capacidade ociosa.
3. A demonstração: antes/depois com OSM/OSRM, economia e CO2 evitado.
4. O impacto: pequenas empresas acessam otimização antes restrita a grandes
   operações.
5. O modelo: SaaS leve, taxa por otimização e relatórios ESG.
6. O fechamento: menos desperdício energético, mais inclusão digital e logística
   local mais sustentável.

## Recomendação Final

A ideia deve seguir, mas com o nome e narrativa ajustados para o regulamento.
O foco do pitch não deve ser "frete mais barato" como tese principal. A tese
principal deve ser:

> reduzir desperdício energético e emissões na logística local por meio de
> inclusão digital e colaboração operacional entre pequenas empresas.

Com esse enquadramento, a GreenRoute Energia Compartilhada fica alinhada ao
Ideathon Energia para Todos e tem escopo técnico viável para um MVP de
hackathon.
