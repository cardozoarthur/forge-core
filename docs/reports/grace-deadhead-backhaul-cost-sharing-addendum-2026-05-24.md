# GRACE - Addendum de Goal

## Km Morto, Retorno e Rateio Justo de Custos

**Workflow Forge:** `wf_710fe1d41a324dd4b22af04a65f53711`  
**Run Forge:** `run_65dd78cebf3748f4b8b02d75f2079bb3`  
**Status:** goal atualizado em tempo de execução  
**Tema:** Aproveitamento Inteligente de Retorno

## Mudança no Goal

O GRACE deve calcular não só a rota compartilhada, mas também o desperdício
logístico antes e depois da recomendação.

O MVP passa a exigir:

- cálculo de **km morto**;
- cálculo de **km com sobra de capacidade**;
- análise da ida e da volta;
- recomendação de retorno/backhaul;
- taxa justa entre empresas;
- divisão transparente de custos e economia entre cargas.

## Conceitos

### Km Morto

Km morto é a distância percorrida por um veículo sem carga útil.

Exemplo:

- caminhão entrega em Pelotas;
- volta para Arroio Grande vazio;
- esse trecho de volta é km morto.

### Km com Sobra de Capacidade

Mesmo quando o caminhão não está vazio, pode haver desperdício parcial.

Exemplo:

- caminhão tem capacidade para 1.000 kg;
- está carregando 400 kg;
- há 600 kg de capacidade ociosa naquele trecho.

Para o MVP, usar:

```txt
capacidade_ociosa_km = distancia_do_segmento * capacidade_ociosa_percentual
```

Exemplo:

```txt
100 km * 60% livre = 60 capacidade-km ociosa
```

### Retorno / Backhaul

Se um caminhão já vai voltar vazio ou com sobra de capacidade, o sistema deve
procurar cargas compatíveis no sentido de retorno.

Exemplo:

- Empresa A vai Arroio Grande -> Pelotas e volta para Arroio Grande;
- Empresa B precisa enviar carga de Pelotas -> Arroio Grande;
- se o caminhão de A tem capacidade livre na volta, a carga de B deve ser
  candidata para aproveitar esse retorno.

## Decisão Esperada do Sistema

Para cada veículo e trecho, o GRACE deve calcular:

- distância planejada;
- distância colaborativa;
- distância evitada;
- km morto antes;
- km morto depois;
- capacidade ociosa antes;
- capacidade ociosa depois;
- capacidade livre por trecho;
- custo marginal;
- CO2 estimado;
- economia energética;
- taxa justa recomendada.

## Fórmula Simples para o MVP

### Custo Solo

Quanto a empresa pagaria se fizesse a rota sozinha:

```txt
custo_solo = distancia_solo_km * custo_por_km
```

### Custo Marginal do Veículo Parceiro

Quanto custa para o caminhão parceiro aceitar a carga:

```txt
custo_marginal = desvio_km * custo_por_km + custo_de_parada + custo_de_manuseio
```

Se não houver desvio relevante:

```txt
custo_marginal = custo_de_parada + custo_de_manuseio
```

### Economia Total Criada

```txt
economia_total = custo_solo - custo_marginal
```

### Taxa Justa Recomendada

Para o MVP, a taxa pode ser:

```txt
taxa_recomendada = custo_marginal + economia_total * percentual_compartilhado_com_transportador
```

Com limites:

```txt
taxa_recomendada >= custo_marginal
taxa_recomendada < custo_solo
```

Assim:

- quem envia paga menos do que pagaria sozinho;
- quem transporta recebe mais do que o custo marginal;
- a economia é dividida de forma transparente.

## Exemplo de Retorno

Situação:

- Empresa A faz Arroio Grande -> Pelotas e retorna para Arroio Grande;
- Empresa B precisa enviar uma carga de Pelotas -> Arroio Grande;
- o caminhão de A voltaria vazio ou com capacidade livre.

Recomendação do GRACE:

| Item | Resultado |
|---|---|
| Trecho | Pelotas -> Arroio Grande |
| Veículo escolhido | Caminhão A |
| Motivo | já fará o retorno e possui capacidade livre |
| Benefício para B | paga menos do que uma rota dedicada |
| Benefício para A | recebe uma taxa por capacidade que seria desperdiçada |
| Benefício ambiental | reduz km morto e emissão marginal |

## Saída Esperada no MVP

Cada recomendação deve mostrar:

- quem carrega;
- quem envia;
- rota original;
- rota colaborativa;
- trecho de ida ou retorno;
- caminhão escolhido;
- carga transportada;
- km morto evitado;
- capacidade ociosa aproveitada;
- custo solo;
- custo marginal;
- taxa recomendada;
- economia para quem envia;
- receita líquida para quem transporta;
- CO2 evitado;
- justificativa em linguagem simples.

## Critérios de Teste

O MVP deve testar pelo menos:

1. Carga aproveitando ida segmentada.
2. Carga aproveitando retorno de caminhão vazio.
3. Carga rejeitada porque o desvio torna a colaboração injusta.
4. Carga rejeitada porque excede capacidade.
5. Carga aceita com taxa menor que custo solo e maior que custo marginal.

## Linguagem de Pitch

Usar:

- **Aproveitamento Inteligente de Retorno**;
- **redução de km morto**;
- **capacidade ociosa convertida em eficiência**;
- **rateio justo de economia logística**;
- **colaboração sustentável com incentivo econômico**.

Evitar:

- "favor entre empresas";
- "carona de carga";
- "troca improvisada";
- "frete informal".

## Decisão

Essa capacidade aumenta a tese econômica e ambiental do GRACE. O projeto deixa
de apenas reduzir rotas redundantes e passa a transformar capacidade ociosa em
valor compartilhado, com incentivo financeiro justo para todos os participantes.
