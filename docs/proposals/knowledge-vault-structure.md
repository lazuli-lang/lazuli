# Proposta — Estrutura de documento do knowledge vault (v0, file-native)

> **Status:** `## Preview vocab` — 1 piloto destilado (pauta-web `.specs`) + 1 esquema provado (Pleiades AE2 v2). Abaixo da barra de core de ≥3 handlers/≥2 pilotos (`RULE-VOCAB-01`). File/frontmatter-native primeiro; promoção a core depois.
> **Relação:** NÃO reabre gramática. `knowledge-sector-field.md` é a fronteira canônica e já cravou: a ÚNICA gramática é `knowledge <sector>` (escalar); tier/relations/datas/cites são frontmatter+git; as regras são a família `VOCAB-KNOWLEDGE-*`. Esta proposta **preenche** as camadas *file* + *frontmatter* + *sector* que aquele doc deixou como "convenção a destilar do uso real" (`knowledge-sector-field.md:67`).
> **Base:** destilação read-only de `c:\Users\lucas\dev\pauta-web-monorepo\.specs` (taxonomia que cresceu de fato) + esquema fechado provado de `c:\Users\lucas\dev\pleiades` (item/item_version/relation/context_pack/slug/workspace) + ruling do `lazuli-language-architect`.

## Por que cirúrgica (e o que ela NÃO é)

O doc de ouro (`knowledge-primitive-and-specialist-harness.md`) empacotava memória + capataz + 3 projeções + catálogo + decay num corte só — BLOCK no rubric (`grading-rubric.md:301-315`). A extração de gramática já foi feita (`knowledge <sector>`). **Esta proposta é a peça restante e *deliberadamente menor*: o formato concreto do documento de memória.** Zero Rust novo, zero keyword nova, zero engine. Só: o catálogo de setores, o schema de frontmatter, o layout de diretório, e como specs/changes/archive/grading/context aterrissam nele.

A linha que tudo respeita: **a gramática diz a que *setor* uma feature pertence; o *documento* (markdown + frontmatter, versionado por git) carrega tudo o mais.** Nada aqui sobe pro `.lzi`.

---

## 1. Catálogo de SETORES (fechado, opinativo, dev-extensível dentro de limites)

O setor é o slug que aparece em `knowledge <sector>` (gramática) **e** como pasta `knowledge/<sector>/` (arquivo). O catálogo é **fechado e opinativo** — herdando a Rule Zero ("vocabulário sobre mecanismo"): conhecimento de vocabulário aberto vira dialeto por projeto, exatamente o que o `.lzi` proíbe no domínio (`knowledge-primitive-and-specialist-harness.md:52`).

Destilado do que pauta-web **de fato** cultivou (7 categorias observadas, não inventadas) cruzado com o que Pleiades provou como `ItemType` fechado (`doc | decision | rule | prompt | contract | integration`, `item.lzi:51-57`):

### 1.1 Setores CORE (fixos — o default opinativo)

| Sector | Origem destilada | O que captura | Item shape canônico |
|---|---|---|---|
| `decisions` | pauta `ADR.md` (31×) + Pleiades `decision` | Por-que-decidimos: contexto, decisão, **tabela de opções** (Option/Pros/Cons/Why-chosen), consequências | **ADR** |
| `changes` | pauta `changes/<NNN>/` (a unidade de trabalho) + pauta `PRD.md` + `TECH-SPEC.md` (31× cada) + Pleiades `doc` | Uma unidade de mudança rastreável, com lifecycle de status próprio (§4); seus *specs* (PRD: users/roles, user stories Gherkin, out-of-scope; TECH-SPEC: blocos `lazuli`, stubs Go, migration notes) são o **conteúdo** do change, não setor próprio | **CHANGE** (board row + bundle) cujo conteúdo são **PRD** + **TECH-SPEC** |
| `gaps` | pauta `docs/lazuli-gaps*.md` (o único lifecycle maduro) | Lacuna de framework: workaround, primitiva proposta, disposição, doctor-code, wave, branch/commit | **GAP** |
| `lazuli-way` | pauta `pauta-web-architect.md` + `CLAUDE.md` (o método governante) | Doutrina/método: templates, protocolos, regras de dispatch, escape-hatch discipline. Setor global, tier `gold`, puxado em fatias | **DOC** (rule-like) |
| `evaluations` | pauta aponta pra `lazuli-ops` grades; Pleiades não tem | Resultado de grading/rubric: score + threshold + passed (§6). Em pauta vive **fora do repo** — aqui ganha casa file-native | **EVAL** |
| `rules` | Pleiades `rule` + `lazuli-way` adjacente | Invariante/política durável e atômica ("sempre X", "nunca Y") — granularidade menor que `lazuli-way` | **RULE** |
| `contracts` | Pleiades `contract` + `integration` | Contrato de interface / integração externa estável (shape de API, envelope de evento) | **CONTRACT** |

`prompt` e `doc` de Pleiades **não viram setor próprio** aqui: `doc` é absorvido por `changes`/`lazuli-way`; `prompt` é matéria de outro produto (harness), não do vault de conhecimento do projeto. (Pleiades v2 já cortou `marketing`/`media` por raciocínio idêntico — `item.lzi:48-50`.)

### 1.2 Extensão pelo dev (dentro de limites)

O dev **pode** adicionar setores de *domínio do próprio projeto* (ex.: `billing`, `iam`, `media` — espelhando a coluna `Domain` que o pauta-architect carregava: Infra/IAM/Agency/Customer/Billing/Media/Production). Regra:

- Adicionar um setor = criar a pasta `knowledge/<sector>/` **e** referenciá-la via `knowledge <sector>` em ≥1 feature. As duas coisas são amarradas por `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` (grammar ↔ file).
- O setor novo herda o **mesmo** schema de frontmatter (§2) e o **mesmo** lifecycle de tier (§4). Dev estende o *eixo* (que setores existem), **nunca** o *vocabulário interno* (tiers, relation kinds, evidence kinds, confidence — tudo §2/§7 permanece fechado).
- Setor é slug `kebab-case`, sem namespace de vendor (§8).

**Bounded vs dev-extensible em uma frase:** o *conjunto de tiers/relations/evidence-kinds/confidence é fechado e imutável*; o *conjunto de setores é fechado-por-default mas dev-extensível por domínio*; tudo que é vendor/produto fica **fora** (§8).

---

## 2. O DOCUMENTO = markdown + frontmatter

Um documento de conhecimento é **um arquivo `.md`** com bloco de frontmatter YAML no topo. O frontmatter é o schema; o corpo markdown é o conteúdo livre (com o *item shape* sugerido por setor — §4.4).

### 2.1 Schema de frontmatter (o contrato)

O contrato mínimo já está cravado em `knowledge-sector-field.md:28` (`tier | supersedes | revalidate_by | cites | tags`). Esta proposta o **concretiza** com tipos e os campos adicionais que pauta+Pleiades provaram necessários, mantendo todos os catálogos fechados de Pleiades verbatim:

```yaml
---
# --- IDENTIDADE (obrigatório) ---
title:         "Agency Foundation"          # humano, casa com o título do corpo
slug:          agency-foundation            # kebab, casa com o nome do arquivo (sem o NNNN)
sector:        decisions                     # DEVE casar com a pasta pai (doctor-checked)

# --- CURADORIA / QUALIDADE (obrigatório) ---
tier:          approved                      # FECHADO: draft | approved | gold | deprecated
                                             #   (= VersionStatus de Pleiades, item_version.lzi:50-54)
                                             #   deprecated é TERMINAL (sem volta)

# --- DATAS (obrigatório; ISO yyyy-mm-dd) ---
created:       2026-05-28                     # dia/mês/ano de autoria (imutável)
updated:       2026-05-29                     # dia/mês/ano da última edição substantiva

# --- PROVENIÊNCIA (obrigatório) ---
source:        ai                             # FECHADO: human | ai | imported
                                             #   (= Confidence de Pleiades, item.lzi:59-63)
                                             #   source: ai EXIGE >=1 cite antes de sair de draft (§7, AI-CITATION)

# --- RELAÇÕES (opcional) ---
supersedes:    decisions/0007-old-agency     # = relation.replaces (relation.lzi:22-27); sector/slug
cites:                                        # = EvidenceRef[] (item.lzi:33-46); cada cite tem um kind FECHADO
  - { kind: code,  ref: "app/features/agency/agency.lzi", commit: "cb672dc4" }
  - { kind: item,  ref: "changes/0003-agency-prd" }
  - { kind: doc,   ref: "docs/lazuli-gaps.md" }
  - { kind: url,   ref: "https://..." }
  # EvidenceKind FECHADO: code | url | item | issue | comment | doc

# --- FACETAS (opcional, LIVRE) ---
tags:          [agency, core, iam]            # text[] livre; AND-containment; NÃO há catálogo global
                                             #   (Pleiades: tags free-form + suggested_tags advisório por slug)

# --- DECAY / FRESCOR (opcional; obrigatório quando tier: gold) ---
revalidate_by: 2027-05-28                     # ISO date OU duração ISO-8601 (ex.: P180D)
                                             #   gold vencido => VOCAB-KNOWLEDGE-STALE-001
decay_profile: stable                         # FECHADO opcional: stable(730d) | seasonal(180d) | volatile(30d)
                                             #   (= DecayProfile de Pleiades, item.lzi:97-100)

# --- GRADING (obrigatório SOMENTE para sector: evaluations) ---
score:         7.5                            # número
threshold:     9.0                            # número (a barra)
passed:        false                          # bool; tipicamente score >= threshold (§6)
---
```

### 2.2 Tabela de mapeamento (frontmatter ↔ conceito Pleiades provado ↔ catálogo)

| Campo | Conceito Pleiades | Catálogo | Obrigatório |
|---|---|---|---|
| `tier` | `VersionStatus` (`item_version.lzi:50-54`) | **fechado** `draft\|approved\|gold\|deprecated` | sim |
| `created` / `updated` | — (pauta não tinha; net-new, suprindo a lacuna §observada) | ISO `yyyy-mm-dd` | sim |
| `source` | `Confidence` (`item.lzi:59-63`) | **fechado** `human\|ai\|imported` | sim |
| `supersedes` | `relation.replaces` (`relation.lzi:22-27`) | ref `sector/slug` | não |
| `cites` | `EvidenceRef[]` (`item.lzi:33-46`) | kind **fechado** `code\|url\|item\|issue\|comment\|doc` | não |
| `tags` | `item.tags` (`slug.lzi:14-19`) | **livre** (sem enum global) | não |
| `revalidate_by` | `last_revalidated` + threshold | ISO date / duração | sim se `gold` |
| `decay_profile` | `DecayProfile` (`item.lzi:97-100`) | **fechado** `stable\|seasonal\|volatile` | não |
| `score`/`threshold`/`passed` | grade pointer (pauta → lazuli-ops) | numérico/bool | sim se `evaluations` |

**Decisão de escopo deliberada — o que fica FORA do frontmatter:** o `ItemStatus` de 6 valores de Pleiades (`candidate`, `pending_contradiction_review` — `item.lzi:81-87`) é maquinaria de integrity-gate (consenso + Gate 1 de contradição) **não** está no contrato; fica fora até esses gates existirem (Part C, ponto 1 da destilação). Idem os relation kinds `depends_on`/`references`/`duplicates`: só `supersedes`(=replaces) tem campo de frontmatter hoje (alinhamento da destilação, ponto 2). Adicionar `depends_on:`/`relates:` é extensão *net-new* de convenção — não desta v0. Idem `RankingProfile`/`TraversalDirection`/`ContextPackDensity`: pertencem a `context` (§5), não ao documento.

---

## 3. Layout de diretório + datas + ordenação

### 3.1 O caminho (cravado em `knowledge-sector-field.md:26-29`)

```
knowledge/<sector>/NNNN-<slug>.md
```

`knowledge/` é **fonte autorada de primeira classe, versionada** (a source-of-truth do vault), na raiz do projeto — NÃO `.lazuli/`, que é o cache interno descartável/gitignored. O **índice derivado** (futuro sqlite-vec) gerado a partir de `knowledge/` é o que mora em `.lazuli/` (regenerável); os `.md` autorados ficam em `knowledge/`.

Exemplo concreto, destilando a uniformidade rígida de pauta (`changes/NNN-name/{ADR,PRD,TECH-SPEC}.md`):

```
knowledge/
├── decisions/
│   ├── 0001-monorepo-scaffold.md
│   ├── 0003-agency-foundation.md
│   └── 0008-billing-config.md           # tier: draft  (era ADR "Proposed")
├── changes/                             # specs (PRD/TECH-SPEC) são conteúdo do change, co-locados pelo NNNN
│   ├── 0003-agency-foundation.prd.md    # PRD e TECH-SPEC compartilham o NNNN do change
│   ├── 0003-agency-foundation.tech.md
│   └── 0029-hoxo-financial-integration.md
├── gaps/
│   ├── 0007-many-through.md             # GAP-07, found-in via cites
│   └── 0042-derived-field.md
├── lazuli-way/
│   └── 0001-escape-hatches.md           # tier: gold, global
├── evaluations/
│   └── 0001-pauta-gaps-bundle.md        # score/threshold/passed
└── rules/
    └── 0001-no-vendor-namespace.md
```

### 3.2 NNNN — ordinal como migration

- `NNNN` = sequência zero-padded (4 dígitos; pauta usava 3 e bateu em 031 — 4 dá folga). **Stable ID + ordem de autoria**, exatamente como pauta usou `NNN` (ID + prioridade) e como migrations ordenam.
- O `NNNN` é o **join key universal** (lição mais forte de pauta): mesmo slug atravessa nome de arquivo, `supersedes:`, `cites:{kind:item}`, e o board. Um `change` e seus `specs` compartilham o `NNNN` (ex.: `0003` cobre o ADR em `decisions/`, o PRD e o TECH no próprio `changes/`).
- Ordenação **dentro do setor** = por `NNNN` (não por data). Pauta provou isso: ordering é por ordinal/wave, não cronológico.

### 3.3 Datas + o modelo git-as-history

Pauta **não tinha** `created:`/`updated:` — só um carimbo global `2026-05-28` repetido inline como anotação de resolução ("Primitive exists as of 2026-05-28"). Isso é frágil; esta v0 corrige adicionando `created`/`updated` ISO ao frontmatter (§2), porque o vault precisa de frescor por-documento (decay) que pauta não exercia.

Mas o modelo de história é o de `knowledge-sector-field.md:31` — **não construir engine**:

| Eixo | Onde vive | Semântica |
|---|---|---|
| **História append-only, datada** | **git** (`git log` de `knowledge/`) | toda mudança datada/assinada/imutável. Tamanho da história ≠ tamanho do contexto. |
| **Estado corrente curado** | **working tree** | projeção "gold": o agente lê a *projeção*, não a pilha (`knowledge-primitive-and-specialist-harness.md:112`). |

Logo: `created`/`updated` no frontmatter são *conveniência de leitura humana + insumo de decay*; a **fonte de verdade** de "quando" é o git. `revalidate_by` é o único campo de data que o doctor consome ativamente (STALE).

---

## 4. Como SPECS / CHANGES / ARCHIVE aterrissam

### 4.1 `changes` — um setor com lifecycle de status

`changes` é o setor mais "vivo". Destilado dos 3 lifecycles paralelos de pauta, **reconciliados num só campo `tier`** mais um `status` de change:

- **Change status** (o board emoji de pauta: `pending → specced → in-progress → archived`, `blocked` como off-ramp) vira o campo de corpo/frontmatter de change. Mapeamento direto para `tier`: `specced` ≈ `approved`; `archived` ≈ `gold` (verdade durável e implementada) **ou** `deprecated` (§4.3); `pending` ≈ `draft`.
- A `STATUS.md` board de pauta (1 tabela, "não editar o status à mão") vira a **projeção derivada** `lazuli inspect --expand=knowledge` (§5) — não um arquivo curado à mão. O dispatch/wave/deps que pauta carregava no board são frontmatter do change (`tags:[wave-1]`, `cites:{kind:item}` para deps com razão no corpo — pauta provou que a razão da dep mora no TECH-SPEC, não no board).

### 4.2 specs — PRD e TECH-SPEC como conteúdo do change (não setor próprio)

Os *specs* são o **conteúdo** de um change (não um setor próprio): dois *shapes* que vivem no setor `changes/` e compartilham o `NNNN` do change (co-locação por ordinal, não por pasta — diferente de pauta que co-locava na mesma pasta `NNN/`):

- **PRD** (`<NNNN>-<slug>.prd.md`): Purpose, tabela Users & roles, user stories Gherkin Given/When/Then, Out-of-scope, Open questions, Success metrics.
- **TECH-SPEC** (`<NNNN>-<slug>.tech.md`): blocos `lazuli` (resource/command/query/event/experience), stubs Go, test cases, migration notes, `## Dependencies` (com razão por dep), e o `## Lazuli gap log` (a join-table change↔gap).

### 4.3 ARCHIVE — tier `deprecated`, NÃO uma árvore `archive/`

**Decisão cravada (reconciliando a evidência):** pauta tinha `archive/` como pasta-terminal *definida mas nunca exercida* (0 arquivos) — completion era "mover o dir pra `archive/`". Isso **conflita** com o modelo git-as-history (§3.3): mover arquivos quebra `git log --follow` e duplica o join-key. Logo:

- **Não há árvore `archive/`.** "Arquivado/obsoleto" = `tier: deprecated` no frontmatter (terminal, `item_version.lzi`). O documento **fica na mesma pasta de setor**; só o tier muda.
- "Concluído e ainda verdadeiro" = `tier: gold` (não some; é a verdade corrente). "Concluído e superado" = `tier: deprecated` + `supersedes` apontando pro sucessor.
- A história ("estava ativo, virou deprecated em D") é o `git log`, não a localização no disco. Isto **resolve** a tensão que pauta deixou em aberto (archive definido-mas-vazio): o terminal-state vira *estado de tier*, não *posição de diretório*.

ADR/PRD/TECH-SPEC são, portanto, **item shapes** (templates de corpo) dentro dos setores `decisions`/`changes`, não tipos de primeira classe. O *shape* é convenção de template; o *setor* + *tier* são o que o doctor enxerga.

---

## 5. CONTEXT — os dois eixos (palavra-chave futura, NÃO `pack`)

`knowledge-sector-field.md:61` já reservou: o eixo-propósito é **`context <name>`** — **NUNCA `pack`** (ocupado pelo registry, `grammar.registry.md`). Esta v0 **não** implementa `context` (é proposta própria, forward-looking); apenas especifica como o documento se presta a ele, herdando o catálogo fechado provado de Pleiades `context_pack`:

Um `context` é **uma query nomeada sobre setores + tags + tier** (não um saco de docs taggeado à mão). Dois eixos (`knowledge-primitive-and-specialist-harness.md:90-96`):

- **Eixo propósito (durável):** ex. `onboarding` — assemblado por significado. `includes sector [...] / where tier >= approved / traverse ... / rank by_tier`. Os catálogos que ele consome são fechados-Pleiades e **não** moram no documento (são query-param): `ContextPackProfile` (`default`=approved+gold | `strict`=gold-only | `debug`=tudo, `context_pack.lzi:85-88`), `ContextPackDensity` (`full`|`summary_only`|`index_only`, `:99-103`), `RankingProfile` (`default`|`by_priority`|`by_recency`|`by_tier`), `TraversalDirection` (`outbound`|`inbound`|`both`).
- **Eixo tempo (handoff):** a "passagem de bastão" — projeção de *run-memory* recente, assemblada por recência, pro agente seco recapturar o estado do fluxo. É aqui que `created`/`updated` (§2) ganham tração: o handoff ordena por `updated` desc.

**Vantagem injusta:** a memória é o compilador — o `context` é *derivado* do grafo IR + vault (`inspect` já projeta purposes/uses/events), não taggeado à mão. Por isso `context` é forward-looking e mora numa proposta própria; **este documento só garante que o frontmatter (§2) tem os campos que essa query vai consumir** (`sector`, `tier`, `tags`, `updated`, `supersedes`).

---

## 6. GRADING — como itens de evaluation vivem

Pauta guardava só *ponteiros* para grades externas (lazuli-ops: "graded 7.5 (BLOCK)"); Pleiades não modela grade. Esta v0 dá casa file-native ao setor `evaluations`:

- Um item de `evaluations` é um `.md` com frontmatter trazendo o trio **`score` (número) + `threshold` (número) + `passed` (bool)**.
- **O tier reflete a nota** (a reconciliação pedida): `passed: true` (score ≥ threshold) ⇒ elegível a `tier: approved`/`gold`; `passed: false` (ex.: 7.5 < 9.0, "BLOCK") ⇒ `tier: draft` (não promove). Isto materializa "PBQ grada com vibe; Lazuli grada com o compilador" (`knowledge-primitive-and-specialist-harness.md:143`): a coluna "Passed" é fato, não vibe.
- O corpo cita o que foi avaliado via `cites:` (kind `item`/`code`/`doc`) — ex. o change ou a proposta sob grade. Threshold default sugerido pelo método (lazuli-ops usa ≥9.0 pra wave; ≥8.5 pra adoção em core) mora em `lazuli-way`, não hardcoded no item.

`score`/`threshold`/`passed` são **opcionais globalmente, obrigatórios em `evaluations`** — não poluem os outros setores.

---

## 7. Modelo GATED-WRITE (anti-lixão) — e qual `VOCAB-KNOWLEDGE-*` força cada peça

O modo de falha nº1 de memória é o **lixão**: pasta gigante de notas não-validadas, stale, redundantes — o agente afoga (`knowledge-primitive-and-specialist-harness.md:109`). Escrita de memória **deixa de ser ação grátis**. A cura é a disciplina de migration, e **cada peça é forçada por uma regra `VOCAB-KNOWLEDGE-*` já cravada em `knowledge-sector-field.md:38-44`**:

| Disciplina | Como funciona | Regra que força |
|---|---|---|
| **Setor existe** | um doc só existe sob `<sector>/` referenciado por `knowledge <sector>` | `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` (grammar ↔ file) |
| **draft → gold gated** | nada nasce `gold`; tem que ter passado por `draft` no git (catálogo + dedup + durável + citado) | `VOCAB-KNOWLEDGE-UNGATED-WRITE-001` (file + git) — porta o `AI-GOLD-BLOCK` de Pleiades |
| **Citação (AI)** | `source: ai` exige ≥1 `cite` antes de promover (Gate `AI-CITATION-001` de Pleiades) | coberto por `UNGATED-WRITE` (cite é pré-condição do gate de promoção) |
| **Decay / frescor** | `gold` sem reafirmar vence `revalidate_by` → vira stale, some da projeção | `VOCAB-KNOWLEDGE-STALE-001` (file) — porta o `AGING-DECAY-001` |
| **Cite resolve** | `cites:` não pode apontar símbolo/arquivo fantasma | `VOCAB-KNOWLEDGE-DANGLING-CITE-001` (file ↔ IR) |
| **Supersede, não acumule** | uma verdade por tópico: dois `gold` no mesmo setor sem `replaces`/`deprecated` é proibido | `VOCAB-KNOWLEDGE-DUP-TOPIC-001` (file) — porta o single-gold invariant |

Curadoria **é paga pelo roteiro** (hooks/capataz), não por disciplina manual que perde pra entropia (`knowledge-primitive-and-specialist-harness.md:115`). As 5 regras são o sensor exit-code 0/1; nenhuma nova é necessária para esta estrutura — ela foi *projetada* contra o contrato de doctor já cravado.

---

## 8. BOUNDED vs DEV-EXTENSIBLE vs OUT

| Eixo | Regime | Detalhe |
|---|---|---|
| **tier** | **fechado/imutável** | `draft\|approved\|gold\|deprecated` — verbatim Pleiades. Dev não adiciona tier. |
| **source/confidence** | **fechado/imutável** | `human\|ai\|imported`. |
| **cites.kind** | **fechado/imutável** | `code\|url\|item\|issue\|comment\|doc`. |
| **decay_profile** | **fechado/imutável** | `stable\|seasonal\|volatile`. |
| **relation (supersedes)** | **fechado** | só `replaces` tem campo hoje; `depends_on/references/duplicates` são extensão futura de convenção, não v0. |
| **context profile/density/rank/traversal** | **fechado** | catálogos de query (§5), não do documento. |
| **sectors** | **fechado-por-default, DEV-EXTENSÍVEL por domínio** | core §1.1 + setores de domínio do projeto (§1.2). Slug kebab, sem vendor. |
| **tags** | **LIVRE** | sem enum global (Pleiades + contrato concordam). Opcional: `suggested_tags` advisório por setor. NUNCA hard enum. |
| **score/threshold/passed** | numérico/bool | só `evaluations`. |
| **OUT — vendor/produto** | **fora do core** | nenhum setor/tag/campo nomeado por vendor ou produto (Stripe, MercadoPago, etc.). Espelha `@plugin/<name>` ≠ core, e a regra do CLAUDE.md "nunca `@plugin/PautaWebMonorepo/<provider>`". Specificidade de produto vive em `handlers/`/plugin, não no vault. |
| **OUT — integrity-gate de 6 estados** | fora desta v0 | `candidate`/`pending_contradiction_review` + relation `contradicts` só quando os gates existirem (Pleiades Phase-B). |

---

## Autocrítica adversarial

- **Maior risco — o setor `changes` reabre o pântano dos 3-lifecycles.** Pauta tinha 3 axes paralelos (change-status, ADR-status, gap-disposition) e *só o gap-disposition foi exercido de verdade*; os outros congelaram num único valor (`specced`/`Accepted`). Comprimir tudo em `tier` (§4.1) é elegante no papel mas **pode perder o status operacional de change** (in-progress vs blocked vs specced) que `tier` não distingue (todos viram `approved`). Se o roteiro precisar do board emoji, vai querer um `status:` separado — e aí o frontmatter cresce de novo. **Provar primeiro:** que `tier` + `tags:[wave-N]` + `git` cobrem o que a `STATUS.md` cobria, OU admitir um `status:` fechado adicional só em `changes`.

- **Over-scoped — `evaluations`, `contracts`, `rules` como setores core.** Pauta **não** tinha `evaluations` no repo (era ponteiro externo); Pleiades **não** tinha grade. Promover `evaluations`/`contracts`/`rules` a core §1.1 é extrapolar além do destilado — viola o próprio mandamento "não invente taxonomia, destile". O defensável-por-evidência é **4 setores** (decisions, changes, gaps, lazuli-way) — os *specs* (PRD/TECH-SPEC) são conteúdo do `changes`, não setor próprio. Os outros 3 deveriam ser **dev-extensíveis (§1.2), não core**, até ≥1 piloto exercê-los. Esta é a falha mais provável num grade.

- **O que provar primeiro (ordem de prova, `knowledge-primitive-and-specialist-harness.md:178`):** (1) migrar o `.specs` de pauta para `knowledge/{decisions,changes,gaps}/` (os specs viram conteúdo do `changes`) com este frontmatter e ver se o join-key `NNNN` + `cites` substitui a `STATUS.md` board sem perda; (2) rodar as 5 regras `VOCAB-KNOWLEDGE-*` contra o resultado e confirmar zero falso-positivo; (3) só então decidir se `evaluations`/`rules`/`contracts` sobem a core. Tudo file-native + git + grep — zero Rust, zero índice vetorial — antes de qualquer promoção.
