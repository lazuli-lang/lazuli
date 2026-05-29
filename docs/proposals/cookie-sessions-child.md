# Proposta — `cookie`: o filho de transporte de `auth.sessions` (v0, cirúrgico)

> **Status:** `## Preview vocab` — evidência de **1 piloto** (Hostpoint), abaixo da barra de core de ≥3 app shapes (`RULE-VOCAB-01`, `scope-discipline.md:46,110`). File/feature-native primeiro, promoção a core depois.
> **Relação:** complementa a primitiva `auth.sessions` (que modela *lifetime + rotation* e nada de transporte) — mesma forma que `knowledge <sector>` complementou `purpose`/`non_goals`/`attach_ctx` sem inchar a primitiva.
> **Base:** ruling do `lazuli-language-architect` (2026-05-28), grounded em `file:line`.

## Por que cirúrgica

A tentação é empacotar "cookie + revogação + CHIPS + multi-domain + prefixos `__Host-`" num corte só — exatamente o anti-padrão "seis primitivas em uma cut" que levou BLOCK no rubric (`grading-rubric.md:301-312`). Aqui fica **só o que é gramática**: um filho com catálogo fechado de atributos de transporte. Decisão de revogação, CHIPS, multi-domain e rotação de nome têm casa fora do core e ficam fora.

A linha que tudo respeita: o filho `cookie` declara **como o cookie é carimbado** (atributos de transporte, invariantes entre apps); tudo que **decide quando/quais sessões** carimbar ou revogar fica no handler.

## O que é GRAMMAR (mínimo — 1 filho, catálogo fechado, 0 lógica nova)

Slot irmão de `ttl`/`access_ttl`/`rotation` sob `sessions`. Cada eixo `Option<_>` com default do framework — idêntico a como `RotationConfig` já trata seus slots (`crates/lazuli_ir/src/nodes/auth.rs:140-157`).

**Authoring (`.lzi`):**
```lazuli
sessions
  resource UserSession
  ttl         "7 days"
  access_ttl  "15 minutes"
  cookie                            # ← ÚNICA adição: filho de transporte
    name        "lazuli_session"    # opcional, default lazuli_session
    same_site   lax                 # catálogo FECHADO: lax | strict | none
    secure      true                # default true (SEC-H1)
    http_only   true                # default true
    domain      ".example.com"      # opcional
    path        "/"                 # default /
```

**IR (`auth.rs` — novo struct + slot opcional):**
```rust
pub struct SessionCookie {
    pub name:      Option<String>,
    pub same_site: Option<String>,  // closed: lax|strict|none — doctor-checked
    pub secure:    Option<bool>,
    pub http_only: Option<bool>,
    pub domain:    Option<String>,
    pub path:      Option<String>,
    pub span_ref:  Option<SpanRef>,
}
// em AuthSessions: pub cookie: Option<SessionCookie>,  // None => literais runtime de hoje
```

- **Catálogo fechado reusado, NÃO criado.** `same_site = lax | strict | none` já existe em `crates/lazuli_keywords/src/registry.rs:3227-3229` (valores `Context::Cookie`) e no doctor de `CookieProfile` (`security.rs:162`). O filho `cookie` **reusa** esse catálogo — zero vocab de valor novo.
- **Opção pura, sem expansão.** Nada de `tier`/`relates`/`partitioned`/`prefix` como sub-bloco — fere Determinism (`grading-rubric.md:132`, "um jeito de dizer cada coisa") e vira config-em-DSL.
- **Back-compat garantido.** `cookie` ausente ⇒ `None` ⇒ runtime usa os literais hardcoded de hoje. Apps existentes não mudam.

## Wire-not-reimplement — o lowering é fluxo, não lógica

O runtime JÁ faz todo o I/O de cookie. Os 6 eixos apenas substituem literais hardcoded via o sink `CookieOpts` existente:

- `SetSessionCookie` carimba hoje `Path:"/"`, `AllowJS:false`, `Secure: sessionCookieSecureDefault()`, `SameSite: http.SameSiteLaxMode` (`runtime/go/lazuli/ctx.go:115-121`); o gêmeo `SetRefreshCookie` repete em `:138-144`.
- O sink `CookieOpts{TTL,Path,Domain,AllowJS,Secure,SameSite}` é wire-fino, 82 LOC tudo `net/http` (`runtime/go/lazuli/http_cookies.go:14-21,36-49`).
- Lowering: cada eixo declarado preenche o literal correspondente. `http_only=true` mapeia para `AllowJS:false` (inverso, conforme `ctx.go:118` + `http_cookies.go:44`). Zero lógica de transporte nova.

## O que fica RUNTIME / escape-hatch (a cauda de 20%)

Confirmado contra a evidência Hostpoint (8 pontos de toque): a **mecânica** do cookie tem variação ZERO (4 SET byte-idênticos, 3 READ idênticos, 1 CLEAR nullário — é vocabulário). A **decisão** que varia é só o *escopo de revogação*, expresso como `WHERE` SQL. Logo:

| Fica fora do core | Onde mora | Evidência |
|---|---|---|
| **Escopo de revogação** (per-device vs everywhere) — a DECISÃO de produto | `@fn.<name>` handler (escape hatch #1) | `WHERE id = $1` vs `WHERE "user" = $1` em `logout.go:25-39`; `revoke_session.go`; `revoke_other_sessions.go:23-28` |
| **Roteamento/redirect/status HTTP** dos fluxos OAuth | handler | `login_with_google.go` retorna 404 para a SPA rotear |
| **CHIPS / `Partitioned`** | override / plugin | campo inexistente em `http.Cookie` que o runtime usa (`http_cookies.go:37-47`); forçaria runtime à frente da stdlib |
| **Prefixos `__Host-` / `__Secure-`** | **doctor lint** sobre eixos fechados, não vocab | invariantes deriváveis (`__Host-` constrange path=/, sem domain, secure=true) |
| **Multi-domain / per-environment** | edge-app / manifesto via capability `cookie_domain` | `auth-refresh/happy.lzi:18` + `rule_009_cookie_domain` (`rules.rs:158-166`) |
| **Rotação de NOME / migração dual-read** | `main.go` do app (escape hatch #5) | topologia operacional, não eixo declarativo |
| **Emissão do token** (`auth.IssueSession`/`RotateSession`) | runtime | handler só decide *quando* chamar |

**Parity afirmada (Probe P-B, `grading-rubric.md:936-951`):** após o filho `cookie` aterrissar, `handler @fn.X` continua first-class para o mesmo command/query/job. O filho `cookie` NÃO deprecia, NÃO marca handler como "legacy", e NÃO emite `should-be-declarative`. Zero hits de `deprecat|legacy|prefer.*vocab`.

## Doctor (regras a adicionar)

| Código | Dispara quando | Lado validado |
|---|---|---|
| `SESSION-COOKIE-INSECURE-IN-PROD-001` | `secure false` (ou default rebaixado) com profile de deploy `production` | grammar ↔ profile |
| `SESSION-COOKIE-SAMESITE-NONE-INSECURE-001` | `same_site none` sem `secure true` (browsers rejeitam `SameSite=None` sem `Secure`) | grammar (cross-axis) |
| `SESSION-COOKIE-MISSING-001` | `auth.sessions` com `rotation`/refresh ativo mas nenhum `cookie` declarado E nenhum `app.cookie` cobrindo o cookie de sessão | grammar ↔ grammar |
| `SESSION-COOKIE-PROFILE-CONFLICT-001` | mesmo eixo ditado por `app.cookie`/`CookieProfile` E por `auth.sessions.cookie` com valores divergentes, sem precedência resolvível | feature ↔ app-manifest |
| `SESSION-COOKIE-HOST-PREFIX-VIOLATION-001` | `name "__Host-…"` com `domain` setado, ou `path != "/"`, ou `secure false` (invariante do prefixo) | grammar (derived invariant) |

Família reusa a categoria de hygiene de cookie já existente (`CookieProfile` doctor em `security.rs:151-170`). Nenhum código acima existe hoje — todos são net-new sob este `## New diagnostics` (mecânica grep de `grading-rubric.md:136`).

## Faces de surface-parity tocadas (é keyword nova)

`cookie` é um statement novo em `Context::Feature` (sub-`sessions`), então atravessa **todas** as faces canônicas da camada de linguagem (`architecture.md:104`). Inventário obrigatório:

| Face | Crate / arquivo | Mudança |
|---|---|---|
| **Lexer + parser** (`.lzi`) | `crates/lazuli_syntax/.../auth/sessions.rs` | aceitar `cookie` no `else` de rejeição (hoje `:85-89`); parsear os 6 eixos |
| **Keyword registry** | `crates/lazuli_keywords/src/registry.rs` | registrar `cookie` em `Context::Feature` + os 6 eixos no novo contexto (ver risco abaixo) |
| **IR** | `crates/lazuli_ir/src/nodes/auth.rs` | `struct SessionCookie` + slot `cookie: Option<_>` em `AuthSessions` |
| **Doctor** | `crates/lazuli_doctor*` | as 5 regras `SESSION-COOKIE-*` |
| **Inspect** | `crates/lazuli_cli/.../inspect/expand_set.rs` | projetar `cookie` sob `--expand=security` (envelope de transporte da sessão) |
| **Grammar doc** | `docs/grammar.lzi.md:852-866` | adicionar `cookie_block` ao `sessions_body` |
| **Syntax highlighting** | scopes em `registry.rs` | herda `constant.language.cookie.lazuli` (catálogo já existe) |

Nota de fronteira: as faces de **superfície de audiência** do `CLAUDE.md` (`.lzx` / `.web.lzx` / `.mobile.lzx`) **NÃO são tocadas** — cookie é transporte HTTP-edge e nunca chega a uma surface. Isso é feature, não lacuna: zero vazamento de "transport mechanics" para projeção multi-target (Multi-target fit, `grading-rubric.md:134`).

## Precedência e escopo de keyword — a decisão de design (RESOLVIDA)

Existem hoje **dois lugares** que podem ditar atributos do cookie de sessão: o perfil de hygiene app-wide `app.cookie` / `CookieProfile` (`security.rs:143-170`) e — com esta proposta — o novo `auth.sessions.cookie` feature-level. Há também a capability `cookie_domain` no edge de refresh (`auth-refresh/happy.lzi:18` + `rule_009_cookie_domain`).

**Nuance que agrava (e que a evidence-brief errou levemente):** os eixos `secure`/`http_only`/`same_site`/`domain`/`path` existem hoje **exclusivamente** como statements `Context::Cookie` (`registry.rs:932-973`), sob a SECTION `cookie` que é `Context::App` (`registry.rs:754-758`). `name` só existe como `modifier` genérico (`registry.rs:3031`). Ou seja: hoje esses nomes vivem **só no manifesto-app**, não na feature. Introduzir os mesmos nomes em `Context::Feature` cria ambiguidade de escopo **no próprio keyword-registry**, além da precedência semântica.

**Precedência — CRAVADA (single-source):**
- `app.cookie` / `CookieProfile` ⇒ default de **higiene HTTP-edge app-wide**.
- `auth.sessions.cookie` ⇒ override **apenas** do cookie de **sessão/refresh** (`name`/`path`/`same_site`/`secure`/`http_only`/`domain`).
- Quando ambos tocam o mesmo eixo do cookie de sessão: **`auth.sessions.cookie` vence** (override mais específico); divergência irreconciliável ⇒ `SESSION-COOKIE-PROFILE-CONFLICT-001`.
- **No registry: opção (b) — cravada.** O filho de feature **reusa o mesmo `Context::Cookie` já existente**, apenas **ancorado pelo pai `sessions`** (em vez de pelo `app.cookie`). Um único contexto de vocab para atributos de cookie, **duas posições de âncora** (app-manifest + `auth.sessions`) — zero binding duplicado, zero ambiguidade de escopo no keyword-registry, zero "duas formas de dizer o mesmo eixo". A opção (a) (binding disjunto em `Context::Feature`) é **rejeitada**: duplicaria os nomes em dois contextos, o oposto da Rule Zero. Implementação: `registry.rs` ancora o `cookie_block` de `sessions` ao mesmo `Context::Cookie`; o parser de `auth/sessions.rs` despacha `cookie` para o mesmo walker de atributos que o `app.cookie` já usa.

## Fronteira (resumo)

| Camada | O quê |
|---|---|
| **Grammar** | filho `cookie` + 6 eixos (catálogo `same_site` reusado, fechado) |
| **Runtime** | I/O de cookie (já existe); lowering só preenche literais via `CookieOpts` |
| **Escape hatch** | escopo de revogação (`@fn` #1), rotação de nome / dual-read (`main.go` #5) |
| **Doctor** | 5 regras `SESSION-COOKIE-*` (insecure-in-prod, samesite-none, missing, profile-conflict, host-prefix) |
| **Inspect** | `--expand=security` projeta o envelope de transporte da sessão |
| **OUT do core** | CHIPS/`Partitioned`, prefixos `__Host-`/`__Secure-` (viram lint), multi-domain (edge-app), qualquer eixo vendor/país/produto (`scope-discipline.md:126`) |

## Preview vocab — provência (`RULE-VOCAB-01`)

Hoje: **1 piloto** (Hostpoint, `app/features/account/handlers/`). A barra de *gatilho* (≥3 handlers de mecânica idêntica, variação zero) está batida com folga — 4 SET + 3 READ + 1 CLEAR. Mas a barra de *adoção em core* (`scope-discipline.md:46,110`) exige **≥3 app shapes distintos** + proposta architect-graded ≥8.5. Por isso entra **`## Preview vocab`**, mesmo precedente de `knowledge-sector-field.md:65-67` (1 piloto → marcador até ≥3 handlers/≥2 pilotos). O catálogo de atributos se destila do uso real antes de promover a core.
