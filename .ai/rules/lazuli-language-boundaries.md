---
name: Lazuli language boundaries
description: The hard separation between Lazuli (contract), Drusa (runtime/codegen), and adapters (provider concrete). Reject any proposal that violates it.
---

Lazuli has three sibling layers. Mixing them is the most common
failure mode in DSL design.

| Layer | Owns | Examples |
|---|---|---|
| **Lazuli** | Verifiable contracts: `.lzi` / `.lzx` source, IR, doctor, inspect, LSP, syntax highlighting. | `command create`, `route id: ID`, `agent summarize_customer`, `policy @policy.update` |
| **Drusa** | Runtime/codegen/wiring: Go scaffolding, dependency injection mechanics, generated transport bindings, prompt-template loading, broker clients. | `func CreateCustomer(ctx, in) error`, generated HTTP server, generated SQL, LLM transport |
| **Adapters** | Concrete provider implementations: HTTP, gRPC, Kafka, NATS, MercadoPago, Stripe, OpenAI, Anthropic, AWS, GCP, Envoy, K8s. | `@drusa/mercadopago`, `@plugin/acme/serasa`, `@adapter.crm` |

## Inviolable rules

1. **No provider names in core syntax.** No `stripe`, `mercadopago`,
   `openai`, `aws`, `kubernetes` keywords. Provider references go
   through registry adapter slots (`@drusa/...`, `@plugin/...`,
   `@adapter.<local>`).

2. **No DI mechanics in source.** Construction order, lifetimes,
   logger/db/client instances, test doubles — all Drusa. The language
   declares `requires integration <slot>: <Capability>` and bindings,
   not `new()` or `inject()`.

3. **No transport mechanics in contracts.** `contract.lzi` declares
   schema, operation, event. It doesn't declare HTTP method routing
   tables, gRPC stub generation flags, broker partition strategies.

4. **No SDK generation as a language concept.** SDK exports for
   Python/TypeScript clients are an *artifact* of contracts, not a
   language feature.

5. **`workspace.lzi` is optional.** A single-app project never needs
   it. Reject any proposal that makes it mandatory.

6. **`container.lzi` does not exist** until registry contracts
   demonstrably can't express real plugin/runtime pressure. Today,
   registry can.

7. **Magic discovery requires visibility.** If a filename convention,
   prefix, or directory rule resolves into language semantics, it
   must surface in `lazuli inspect`, `lazuli doctor`, and LSP. No
   silent runtime behavior.

## When you spot a violation

Reject the proposal in line. Do not merge it into a checklist for
"later." The boundary is enforced through deletion, not migration.

## When you're unsure

Ask: "could a Lazuli project still function if Drusa was replaced by
a hypothetical second runtime targeting Rust + Yew + Flutter?" If the
answer is no because the language is leaking Go-specific or
React-specific assumptions, the proposal is at the wrong layer.
