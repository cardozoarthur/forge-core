# GRACE

## Green Routing And Collaborative Efficiency

**Workflow Forge:** `wf_710fe1d41a324dd4b22af04a65f53711`  
**Run Forge:** `run_65dd78cebf3748f4b8b02d75f2079bb3`  
**Nome final do projeto:** GRACE  
**Significado:** Green Routing And Collaborative Efficiency  
**Decisão de viabilidade:** `viable_with_reframe`

## Decisão

GRACE deve seguir como MVP do Ideathon Energia para Todos, mas com uma narrativa
mais precisa do que a ideia inicial. O projeto não deve ser apresentado como um
marketplace genérico de frete. Ele deve ser apresentado como uma tecnologia de
eficiência energética e inclusão digital para pequenas operações logísticas.

Tese central:

> GRACE reduz desperdício energético, combustível e emissões ao coordenar rotas
> colaborativas e capacidade ociosa de veículos usando OSM, OSRM e recomendações
> inteligentes.

## Encaixe no Regulamento

O regulamento favorece tecnologia, inclusão digital, consumo sustentável,
eficiência energética, redução de consumo e redução de pegada de carbono.

GRACE se encaixa quando o foco do pitch é:

- redução do consumo de combustível por consolidação de rotas;
- redução de emissões de CO2;
- menor desperdício de capacidade veicular;
- acesso de pequenas empresas a otimização normalmente restrita a grandes
  operações;
- mudança de hábito: sair de entregas isoladas para planejamento colaborativo;
- demonstração clara com mapa, antes/depois e cálculo de economia.

## Score pelo Regulamento

| Critério | Peso | Leitura para GRACE |
|---|---:|---|
| Aderência ao tema | 10% | Boa com foco em energia, emissões e consumo sustentável |
| Inovação e criatividade | 20% | Forte pela combinação de OSM/OSRM, colaboração e IA |
| Viabilidade técnica | 20% | Forte para MVP com dados simulados e heurística inicial |
| Impacto social e ambiental | 20% | Forte para pequenas empresas e cooperativas locais |
| Modelo de negócio/funding | 10% | SaaS leve, analytics ESG e taxa por otimização |
| Qualidade do pitch | 20% | Forte se demonstrar antes/depois em 5 minutos |

**Score estimado:** 86/100.

## MVP que Deve Entrar em Desenvolvimento

O MVP deve provar o efeito operacional com dados demo e rotas reais calculadas
por OSRM/OSM. Não precisa operar um marketplace real.

Funcionalidades obrigatórias:

- cadastro de demandas com origem, destino, carga, janela de tempo e volume;
- cadastro de veículos/rotas com capacidade ociosa;
- cálculo de rotas isoladas usando OSRM;
- cálculo de rota colaborativa sugerida;
- matching por proximidade, janela, capacidade e desvio máximo;
- painel de quilômetros economizados;
- painel de combustível/energia economizada;
- painel de CO2 evitado;
- mapa de comparação antes/depois;
- tela de pitch/demo com resumo da tese e impacto.

Fora do MVP:

- pagamento real;
- marketplace aberto;
- rastreamento em tempo real;
- contrato jurídico entre empresas;
- integração ERP;
- precificação dinâmica avançada.

## Backlog de Desenvolvimento

### Epic 1 - Base do Produto

- Criar aplicação web do MVP.
- Definir entidades: demanda, veículo, rota, recomendação e cenário.
- Criar dados demo com pequenas empresas locais.
- Criar fluxo principal: cadastrar demanda, cadastrar capacidade, gerar
  recomendação, comparar impacto.

### Epic 2 - OSM/OSRM

- Criar cliente OSRM para distância e duração.
- Calcular rota individual por demanda.
- Calcular rota com desvio colaborativo.
- Persistir distância, duração e geometria simplificada.
- Criar fallback com coordenadas demo caso OSRM externo falhe.

### Epic 3 - Matching Colaborativo

- Filtrar pares compatíveis por janela de tempo.
- Filtrar por capacidade disponível.
- Calcular desvio máximo permitido.
- Calcular score de compatibilidade.
- Gerar recomendação com justificativa legível para pitch.

### Epic 4 - Impacto Energético e Ambiental

- Estimar combustível economizado por km evitado.
- Estimar CO2 evitado por fator configurável.
- Calcular economia financeira aproximada.
- Mostrar comparação isolado versus colaborativo.

### Epic 5 - Interface e Pitch

- Dashboard com mapa e cards de impacto.
- Tela de cenário antes/depois.
- Tela de recomendação com explicação simples.
- Página de pitch com problema, solução, impacto e modelo de negócio.

### Epic 6 - Testes e Validação

- Teste unitário do cálculo de distância/economia.
- Teste unitário do matching.
- Teste de cenário demo completo.
- Checklist de pitch: 5 minutos, 10 slides, critérios do regulamento.

## Primeiro Slice de Desenvolvimento

O primeiro slice deve ser pequeno e testável:

1. Criar modelo de dados demo.
2. Implementar cálculo OSRM para duas rotas isoladas.
3. Implementar heurística simples de consolidação.
4. Mostrar comparação de km, custo e CO2.
5. Criar uma tela de demo com mapa ou lista de rota.
6. Criar testes dos cálculos.

## Critério de Pronto para Entrar em Desenvolvimento

O workflow está pronto para desenvolvimento quando:

- o goal usa GRACE como nome oficial;
- a viabilidade foi marcada como `viable_with_reframe`;
- o pitch está orientado a eficiência energética e CO2;
- o escopo de MVP está limitado;
- o backlog tem epics e primeiro slice;
- o plano OSM/OSRM separa demo seguro de produção futura;
- os próximos passos têm testes definidos.

## Status do Workflow

O workflow deve avançar até `task-017`, que prepara o plano técnico OSM/OSRM do
MVP. Depois disso, a execução entra em validação de MVP/pitch e melhoria
contínua até o prazo com buffer.
