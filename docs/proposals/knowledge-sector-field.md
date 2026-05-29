# Proposta — `knowledge <sector>`: o campo de conhecimento de feature (v0, cirúrgico)

> **Status:** `## Preview vocab` — sem precedente de ≥3 handlers/≥2 pilotos ainda (`RULE-VOCAB-01`). Candidato `language-light`; file-native primeiro, promoção a core depois.
> **Relação:** extrai a ÚNICA mudança de gramática de `knowledge-primitive-and-specialist-harness.md` (que permanece como VISÃO, não como proposta).
> **Base:** ruling do `lazuli-language-architect` (2026-05-28), grounded em `file:line`.

## Por que cirúrgica

O doc de ouro empacotava memória + capataz + 3 projeções + catálogo + decay num corte só — o anti-padrão que já levou BLOCK no rubric (`grading-rubric.md:301-315`). Aqui fica **só o que é gramática**: um campo escalar. Todo o resto é convenção de arquivo + doctor (não-gramática) ou proposta separada.

## Mudança de gramática (mínima — 1 campo escalar, 1 keyword nova)

```lazuli
feature billing
  purpose "Cobrar clientes e reconciliar faturas."
  non_goals "Cálculo de imposto."
  knowledge billing            # ← ÚNICA adição: slug do setor. Escalar.
```

- `knowledge` está **livre** (`crates/lazuli_lsp/src/keywords.rs:63-395` — nunca reservada; o "knowledge RAG" era só sketch de doc, `grammar.lzi.md:972-988`, nunca no lexer).
- Adições: keyword em `keywords.rs`; `LziFeatureKnowledge { sector, span }` em `ast/feature/context.rs` (irmão de `LziFeatureAttachCtx`); campo no IR `ir/nodes/feature.rs` (ao lado de `purpose`/`non_goals`/`attach_ctx`).
- **Opção A pura.** B/C (bloco expansível com `tier`/`relates`/`feeds`) **rejeitadas**: ferem Determinism ("um jeito de dizer cada coisa") e viram config-em-DSL — o pecado do TOML que a visão corta. `tier`/`relations`/`decay` são propriedade do *documento*, não da *feature*.

## Camada de arquivo (provada primeiro, sem Rust novo)

```
knowledge/<sector>/NNNN-<slug>.md
  frontmatter: tier(draft|approved|gold|deprecated) | supersedes | revalidate_by | cites | tags
```

- `knowledge/` é **fonte autorada de primeira classe, versionada** (a source-of-truth do vault), na raiz do projeto — NÃO `.lazuli/`, que é o cache interno descartável/gitignored. O **índice derivado** (futuro sqlite-vec) gerado a partir de `knowledge/` é o que mora em `.lazuli/` (regenerável); os `.md` autorados ficam em `knowledge/`.
- História append-only = **git** (não construir engine). Estado corrente = working tree (gold).
- Precedente reusável: `lazuli examples validate --check-decay` (`crates/lazuli_cli/src/commands/examples.rs:59-61`) já varre artefatos em disco e sinaliza decay.

## Doctor (família `VOCAB-KNOWLEDGE-*`, categoria `Vocabulary`)

Prefixo `VOCAB-` mapeia pra `Vocabulary` (`rule_category.rs:96`), mesma casa de `VOCAB-CONTEXT-*` que já governa `purpose`/`non_goals`/`attach_ctx`.

| Código | Dispara quando | Lado validado |
|---|---|---|
| `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` | `knowledge <sector>` que NÃO é do catálogo core (`decisions`, `changes`, `gaps`, `lazuli-way`), NÃO está declarado em `Lazurite.toml [knowledge.sectors]`, e NÃO tem pasta `knowledge/<sector>/` | grammar ↔ catálogo/file |
| `VOCAB-KNOWLEDGE-UNGATED-WRITE-001` | doc `gold` sem ter passado por `draft` no git (anti-lixão) | file + git |
| `VOCAB-KNOWLEDGE-STALE-001` | `gold` com `revalidate_by` vencido | file |
| `VOCAB-KNOWLEDGE-DANGLING-CITE-001` | `cites:` aponta símbolo inexistente no IR | file ↔ IR |
| `VOCAB-KNOWLEDGE-DUP-TOPIC-001` | dois `gold` no mesmo setor sem `replaces`/`deprecated` | file |

## Inspect

`lazuli inspect --expand=knowledge` projeta, pela 1ª vez, `purpose` + `non_goals` + `knowledge <sector>` + os docs `gold` do setor. Padrão additive trivial em `crates/lazuli_cli/src/commands/inspect/expand_set.rs`. É a peça de "a memória é o compilador".

## Fronteira (resposta à Q5 do doc de visão)

| Camada | O quê |
|---|---|
| **Grammar** | só `knowledge <sector>` (escalar) |
| **File** | tier, relations, decay, cites, supersessão (frontmatter + git) |
| **Doctor** | 5 regras `VOCAB-KNOWLEDGE-*` (cross-check grammar↔file↔IR + gate de escrita + decay) |
| **Inspect** | `--expand=knowledge` |

## Fora do escopo desta v0 (propostas separadas)

- **`context <name>`** (o "context-pack" eixo-propósito) — NÃO usar `pack` (ocupado pelo registry, `grammar.registry.md`). Proposta própria quando provar valor.
- **RAG → plugin (companion):** deletar o sketch `grammar.lzi.md §16` (`source`/`chunk by`/`embedding @adapter`) — custo zero (nunca reservado). RAG passa a `requires integration embedder: Embedder` + `vectorstore: VectorStore` (contratos já existem) + `@plugin/openai-embeddings`/`@plugin/chromadb` + `@fn` de chunk. **Caveat de execução:** o contrato `vectorstore` é EXPERIMENTAL e sem piloto; só declarar a capability genérica como "caminho canônico de RAG" depois que ≥1 piloto (Pleiades v2.2) fechar o loop e passar `doctor` (`RULE-VOCAB-01`).
- **alias `check` → `doctor --fail-on error --fast`** — não mergear (quebra o contrato de exit-code `cli-exit-codes.md`). Meia página, disciplina de CLI.

## Provência (`RULE-VOCAB-01`)

Hoje: 1 piloto (pauta-web `.specs`). **Marcado `## Preview vocab`** até ≥3 handlers/≥2 pilotos. O catálogo de setores se destila do uso real antes de promover a core.
