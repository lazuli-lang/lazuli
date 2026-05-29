# Lazuli — `knowledge` como Primitivo & a Harness Especialista
## Documento de Ouro · visão unificada (proposta para review + grade)

> **Status:** VISÃO / north-star. **Como proposta de linguagem: BLOQUEADA e superseded** (2026-05-28) — empacotava demais e propunha overload de tokens. A mudança de gramática gradeável foi extraída para `knowledge-sector-field.md` (`knowledge <sector>`, Opção A); RAG sai da gramática → plugin. Este doc permanece como narrativa de visão, NÃO como proposta.
> **Origem:** destilado de uma sessão de design (2026-05-28). Une o melhor de Lazuli, Pleiades, Erudito, Orion e lazuli-ops; corta o excesso.
> **Princípio-guia:** **a harness é a linguagem.** Correção por construção, não policiamento.

---

## 0. Contexto — por que agora

"Harness engineering" virou o gargalo do desenvolvimento com IA: o modelo é capaz, mas falha em padrões previsíveis — one-shot hero, vitória prematura ("pronto" sem testar), amnésia entre sessões, teste falso, processo único (sem validador separado), slop acumulado. A indústria está atacando isso com **harnesses genéricas** (scripts que policiam código arbitrário depois do fato).

Lazuli permite inverter: o agente escreve numa linguagem cujas guardrails o **compilador rejeita antes do codegen**. Uma classe inteira de erro vira estruturalmente impossível, e o "sensor que retorna 0/1" (não o agente se autojulgando) já existe como `lazuli check/doctor/test`.

O autor já construiu, separadamente, todas as peças de uma harness — só fragmentadas em N repos. Este documento as **funde** e corta o que era excesso. É o polimento da pedra: o bom entra cravado, o lixo cai fora.

## 1. A tese

**A harness não é um sistema ao lado da linguagem; a linguagem emite a própria harness.** Mainstream = harness que policia código arbitrário (inerentemente complexa, porque não pode assumir convenção). Lazuli = harness **especialista**, simples, porque a linguagem já colapsou o problema. A simplicidade matadora (mentalidade Rails) **só** é alcançável no especialista.

Posição de pioneiro: a primeira linguagem cujo **compilador é o sensor**, cujo **scaffolding é o feed-forward**, e cujo **agente especialista é o validador** — tudo entregue como a harness da própria linguagem. "Convention over configuration" deixa de ser pra humano e passa a ser pra agente.

## 2. Os três elementos irredutíveis

Toda a harness reduz a três coisas — e só estas:

1. **Instruções** (texto) — o que fazer dentro de um passo. *Feed-forward.* Qualquer LLM infere.
2. **Validações** (script, exit-code 0/1) — o passo produziu o artefato certo? *Sensor.* Determinístico.
3. **Roteiro** (código que dirige a ordem e o "não avança sem passar") — *o capataz.*

A **interface model-agnóstica é o exit-code** (Claude, Codex, Gemini — todos falam "rodei, deu 0/1"). Bind em **texto + script**, nunca na API de um modelo. (Foi o acoplamento à API do Claude que prendeu o Orion a um só modelo.) Por isso **não precisamos de TOML**: o roteiro é código, não config — e config-de-pipeline só era necessária quando a ambição era genérica.

## 3. A arquitetura unificada — cinco camadas, fundidas

| Camada | O que é | Onde vive |
|---|---|---|
| **Feed-forward** | scaffolding que ensina o lazuli way | `lazuli new` + doutrina (no bundle) + conhecimento (no projeto) |
| **Sensores** | `check`/`doctor`/`test` (exit-code) | compilador (consolidar, **não** somar comandos) |
| **Roteiro** | capataz determinístico do loop | hooks + Workflow (Claude Code) ou Agent SDK (standalone) |
| **Validador** | `lazuli-app-architect` (o resíduo de gosto que o compilador não checa) | subagente (contexto/missão próprios) |
| **Memória** | `knowledge` — conhecimento setorizado, ranqueado, validado | **primitivo da linguagem** (§4) |

## 4. PROPOSTA CENTRAL — `knowledge` como primitivo

### 4.1 A semente que já existe

Lazuli já tem, pequeno, o embrião disto: `purpose`, `non_goals`, `attach_ctx` — cada feature **se autodescreve** ("pra isso eu sirvo, pra isso não, esse é o contexto que me cerca"). São **sementes de context-pack declaradas no momento de autoria.** Esta proposta é a **evolução** dessas três, promovidas a um primitivo de conhecimento de primeira classe.

### 4.2 Conceitos (mentalidade Pleiades, vocabulário FECHADO)

Herdando a Rule Zero do Lazuli (vocabulário sobre mecanismo), o conhecimento é **catálogo fechado, opinativo, dev-extensível dentro de limites** — porque conhecimento de vocabulário aberto vira dialeto por projeto, exatamente o que o `.lzi` proíbe no domínio. Vocabulário fechado = **qualquer LLM lê e escreve previsível**.

- **sector** (slug) — o setor semântico de um pedaço de conhecimento.
- **tags** — facetas, dentro de catálogo governado.
- **tier** — `draft → approved → gold → deprecated` (a qualidade/curadoria).
- **relations** — `depends_on | references | replaces | duplicates` (fechado).
- **pack** — uma *query* nomeada que monta um context-pack (§4.4).
- **decay/revalidate** — frescor; sem reafirmação, gold → stale, some da projeção.

### 4.3 Superfície da linguagem — DECISÃO ABERTA (campo único vs. expansível)

> Esta é a decisão que o autor delegou explicitamente ao **architect + grader**. Três candidatos:

**Opção A — campo único (mínimo):**
```lazuli
feature billing
  purpose "Cobrar clientes e reconciliar faturas."
  non_goals "Cálculo de imposto; dunning."
  knowledge billing            # setor ao qual a feature pertence
```
*Prós:* zero cerimônia, parse trivial, evolução direta de `purpose`. *Contras:* não expressa tier/relations/packs — insuficiente pra ser a super-camada.

**Opção B — bloco expansível:**
```lazuli
feature billing
  purpose "Cobrar clientes e reconciliar faturas."
  non_goals "Cálculo de imposto; dunning."
  knowledge
    sector billing
    tags [revenue, core]
    tier gold
    relates depends_on auth
    feeds pack onboarding
```
*Prós:* expressa tudo; é a super-camada. *Contras:* superfície maior; risco de virar "config-em-DSL" (o pecado do TOML que cortamos).

**Opção C — híbrido (atalho + bloco opcional):** `knowledge billing` como caminho comum (uma linha); bloco só quando precisa de tier/relations. *Mais Rails:* o comum é trivial, o raro é possível. **Recomendação do autor da proposta:** C, a confirmar pelo architect.

### 4.4 Context-pack — uma query sobre o IR + o vault

Pack é a evolução de `attach_ctx`: não um filtro manual de docs, mas uma **query semântica montada a partir do IR** que o compilador já tem (`inspect` já projeta purposes, `uses`, events, policies). Dois eixos:

- **Eixo propósito** (durável): `onboarding`, `billing-context` — assemblado por significado.
- **Eixo tempo** (handoff): "o estado do fluxo agora" — assemblado por recência, pro agente seco recapturar (a "passagem de bastão").

```lazuli
pack onboarding
  purpose "Tudo que um dev novo precisa pra entender o sistema."
  includes sector [billing, auth, identity]
  where tier >= approved
  traverse depends_on depth 2
  rank by_tier
```
Vantagem injusta: **a memória é o compilador**, então o pack é *derivado* do grafo, não tagueado à mão. Genéricos (Mem0 etc.) são saco de texto; este é cirúrgico.

### 4.5 Knowledge-as-migration (anti-lixão)

O modo de falha nº1 de memória é o **lixão**: pasta gigante de notas não-validadas, stale, redundantes — o agente afoga. (Inclusive a memória do próprio Claude Code peca nisso: escrita grátis, sem gate, sem decay, pedindo pro agente se autocurar.) A cura é a analogia de **migration**:

- **História append-only, datada, nunca editada** = o **git** de `.lazuli/knowledge/`. (Não construir engine; git já é isso.)
- **Estado atual = projeção curada** = a working tree (só gold). O agente lê a *projeção*, não a pilha. Tamanho da história ≠ tamanho do contexto.
- **Escrita GATED** = entra como `draft` num `_inbox`; só promove a `gold` se passar: está no catálogo, não-redundante (dedup), durável, citado. **Escrever memória deixa de ser ação grátis.**
- **Supersessão, não acumulação** (`replaces` + `deprecated`): uma verdade por tópico na visão corrente.
- **Curadoria paga pelo roteiro**, não por disciplina manual (que perde pra entropia).

Layout file-native (prova primeiro, antes de qualquer Rust):
```
.lazuli/knowledge/<sector>/NNNN-<slug>.md
  frontmatter: tier | supersedes | revalidate_by | cites | tags
```

## 5. Convenção como ambiente (como o lazuli way se transmite)

Não se faz o LLM **decorar** o lazuli way; cria-se o ambiente onde o caminho idiomático é o de menor resistência. Quatro trilhos, todos baratos em token:

1. **Exemplos, não regras** — features canônicas que vêm com a linguagem; o `lazuli new` aponta "leia estas 3, imite a forma". Dupla função: já são fixtures de teste.
2. **O diagnóstico é a aula** — a mensagem do `doctor` (porquê + fix idiomático + exemplo de 1 linha) ensina **no instante do erro, e só então**. Custo zero até errar. *Feed-forward entregue como feedback.*
3. **Retrieval, não push** — o "porquê" profundo mora no vault global (lazuli way = setor global, gold), **puxado em fatias** (200 tokens, não 50KB).
4. **O resíduo de gosto → o subagente validador**, com a doutrina no **contexto descartável dele** — a sessão principal do dev não paga esse token.

## 6. O capataz determinístico (o roteiro)

Não envolver o Claude Code numa CLI nova (erro do Orion) — **especializá-lo**. A harness é ~90% configuração de primitivos que já existem:

| Falha do agente | Mecanismo determinístico |
|---|---|
| Pula passos / troca ordem | control-flow em **código** (Workflow / Agent SDK), não em prompt |
| Troca o formato do output | **validação do artefato por script** (porta portátil do "schema") |
| "Pronto" sem testar | hook `Stop` roda `doctor && test` — não-burlável |
| Slop acumulado | hook `PostToolUse` + subagente validador a cada change |

**Grade determinística:** a coluna "Passed" das evaluations não é vibe de LLM (9/10 chutado) — é **fato do compilador**: `doctor` = 0 findings, coverage ≥ limite, contrato cumprido. *PBQ grada com vibe; Lazuli grada com o compilador.*

Bifurcação de entrega: **dev-time** (dentro do Claude Code) = hooks + Workflow, ~zero infra → prova a célula. **Standalone autônomo** (vibe PBQ) = Claude Agent SDK, loop em código próprio. Mesma alma nos dois.

## 7. As três projeções (uma fonte, três janelas)

O grafo de conhecimento, fundido ao compilador, é renderizado pra três plateias:

- **Agente** → context-packs (propósito + handoff). Quem constrói.
- **Dev/você** → a wiki (Pleiades-produto, UI). Quem cura.
- **Recém-chegado** → onboarding visual (a ideia do **Erudito**). Quem entende como tudo se liga — **narrado** (purpose/non_goals), não um mermaid mudo.

## 8. Lapidação — o que entra cravado, o que cai fora

| Projeto | Entra cravado | Cai fora |
|---|---|---|
| **Lazuli** | a pedra (o centro) | — |
| **Pleiades** | a *mentalidade* (setorizado, ranqueado, curado, validado) → o primitivo `knowledge` | infra/servidor/multi-tenant (vira wiki do dev) |
| **Erudito** | a *ideia* (humano entende o sistema) → 3ª projeção | produto-à-parte |
| **Orion** | a *lição* (capataz determinístico, validador separado, o loop) | o superapp / orquestrador genérico |
| **lazuli-ops** | intacto — é o lapidário se disciplinando (gap→proposta→primitivo) | — |

**Cortar fora é metade do trabalho.** Lapidação é subtração: o excesso (infra do Pleiades, TOML, sprawl de comando, genérico do Orion) é o cisco removido pra joia aparecer.

## 9. Disciplina / anti-goals

- **Sem sprawl de comando.** A harness é skill + sensores existentes; o CLI deve *encolher* (consolidar `check`/`doctor`), não crescer. Sem `lazuli ship` como verbo novo.
- **Sem TOML de pipeline.** Roteiro é código.
- **Sem harness genérica.** Especialista, sempre. (A genérica, se existir, é outro produto.)
- **Sem subsistema Rust especulativo.** O primitivo `knowledge` **emerge** do uso real (§10).
- **Sem escrita de memória não-gated.** Tudo passa pelo portão draft→gold.
- **Disciplina de token.** Conhecimento é puxado, não empurrado; doutrina cara vive no contexto do subagente.

## 10. Ordem de prova (construir sem inchar)

1. **Destilar do `.specs` do pauta-web** (o "ouro" piloto) qual é o **catálogo fechado** real: quais sectors, tiers, relations *provaram* valor. Não se chuta — destila.
2. **Provar file-native primeiro**: `.lazuli/knowledge/*.md` + frontmatter + git + grep. Sem índice vetorial (sqlite-vec só quando o grep parar de escalar), sem servidor, sem UI.
3. **Promover a Rust/nativo** via o loop do lazuli-ops (gap → proposta → grade ≥9.0 → wave). A primitiva nasce **justa, não inchada.**
4. **Roteiro dev-time primeiro** (hooks + Workflow); standalone (Agent SDK) só se/quando quiser o produto autônomo.

## 11. Perguntas para o `architect` + `grader`

1. **Superfície de `knowledge`:** A, B ou C (§4.3)? O atalho híbrido fere a Determinism ("um jeito de dizer cada coisa") do rubric?
2. **Subsunção:** `knowledge` deve *substituir* `attach_ctx` (e talvez absorver `purpose`/`non_goals` como campos), ou conviver?
3. **`pack` é primitivo top-level ou uma forma de `query`?** Qual fere menos a composabilidade e o vocabulário fechado?
4. **Catálogo fechado inicial:** quais sectors/tiers/relations entram no default opinativo, e quanto de extensão-pelo-dev é seguro sem virar dialeto?
5. **Fronteira nativo vs file:** o que é declarado na linguagem (`.lzi`) vs o que vive como convenção de arquivo (`.lazuli/knowledge/`)? O `doctor` valida o quê de cada lado?
6. **Multi-target:** a projeção "onboarding" (§7) tem implicação de codegen (Go/React/Expo) ou é puramente uma view derivada do IR?
7. **Rule Zero & founding principle:** algo aqui tenta reimplementar o que stdlib/lib madura já faz (git como história, sqlite-vec como índice)? Onde o "wire, not reimplement" precisa de vigilância?

---

*Fim do documento de ouro. A pedra está lapidada; falta o architect conferir os cortes e o grader dar a nota.*
