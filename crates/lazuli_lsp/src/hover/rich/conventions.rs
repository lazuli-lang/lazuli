//! Rich Markdown hovers for the resource-convention keywords:
//! `conventions [bundle]` opt-in and the `@owner_axis(through: <col>)`
//! FK annotation. See
//! `docs/proposals/ir-resource-conventions-crud.md`,
//! `docs/proposals/ir-resource-conventions-me.md`,
//! `docs/proposals/ir-resource-conventions-owner-scope.md`.

pub(super) fn rich_conventions_hover(keyword: &str) -> Option<String> {
    match keyword {
        // `docs/proposals/ir-resource-conventions-crud.md` §4.4 + the
        // `docs/proposals/ir-resource-conventions-me.md` §4.4 — rich
        // hover for the resource-level `conventions [..]` opt-in. Body
        // begins with the verbatim one-liner from the proposal so the
        // hover surface, the docstring on `Resource.conventions`, and
        // the doctor diagnostic share phrasing. Cells C4 + M3.
        "conventions" => Some(
            [
                "**`conventions`** — Resource-level conventions opt-in: `conventions [<name1>, <name2>, ...]`. Each entry references a closed-catalog convention bundle that auto-synthesizes commands/queries during lowering. Today's catalog: `crud`, `me`. See `docs/proposals/ir-resource-conventions-crud.md` and `docs/proposals/ir-resource-conventions-me.md`.",
                "",
                "**Closed catalog**",
                "- `crud` — auto-synthesizes the 5 canonical CRUD shapes (`create_<r>`, `update_<r>`, `delete_<r>`, `lookup_<r>`, `list_<r>s`).",
                "- `me` — auto-synthesizes one `lookup_my_<r>` query keyed by the active actor (`ctx.User.ID` / `ctx.User.OrgID`), per `ir-resource-conventions-me.md` §5.",
                "",
                "**Example**",
                "```lazuli",
                "resource Customer",
                "  email: @semantic.Email required unique",
                "  name: Text required",
                "  conventions [crud, me]",
                "```",
                "",
                "**Authoring rules**",
                "- Empty list (`conventions []`) is a parse error — omit the slot instead.",
                "- An author-written `command <name>` overrides the synth for that name; the remaining synth entries still emit (per §6 RULE-VOCAB-02).",
                "- Unknown identifiers fail at parse time with doctor code `conventions_unknown`.",
                "",
                "**Inspect**: `lazuli inspect features` annotates each opted-in resource with `(conventions: <bundle>)` and each synthesized command/query with `[conv:<bundle>]`.",
            ]
            .join("\n"),
        ),
        // `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5 +
        // §11.3 — rich Markdown hover for the `@owner_axis(through: <col>)`
        // FK annotation. Body opens with the verbatim one-liner from
        // §11.3 (also surfaced as the `keyword_description` fallback) so
        // the hover surface, the doctor diagnostic phrasing, and the
        // docstring on `Field.owner_axis` all agree. Cell O3.
        "@owner_axis" | "owner_axis" => Some(
            [
                "**`@owner_axis`** — Field-level annotation: `@owner_axis(through: <column>)`. Marks the field as the FK that anchors the resource's ownership chain. The crud / me synth passes use this to emit ownership-restricted WHERE clauses (the row's owner is resolved through the chain to `ctx.User.ID` rather than just the tenant). See `docs/proposals/ir-resource-conventions-owner-scope.md` §7.",
                "",
                "**Required parameter**",
                "- `through: <column>` — column on the FK target resource that holds the actor key. Typically `user` (the User-typed column on the target).",
                "",
                "**Example**",
                "```lazuli",
                "resource Property",
                "  org: Org required",
                "  host: Host required @owner_axis(through: user)",
                "  name: Text required",
                "  conventions [crud]",
                "```",
                "",
                "**Lowered SQL** (for `delete_property` under `conventions [crud]`):",
                "```sql",
                "DELETE FROM \"property\"",
                "WHERE id = $1",
                "  AND org_id = $2",
                "  AND host IN (SELECT id FROM \"host\" WHERE \"user\" = $3)",
                "```",
                "",
                "**Authoring rules**",
                "- Only valid on FK fields (the type must reference another resource). Doctor code `owner_axis_on_non_fk` fires otherwise.",
                "- The `through:` column must exist on the FK target resource. Doctor code `owner_axis_unknown_through` fires otherwise.",
                "- Redundant with `user: User required unique` on the same resource. Doctor code `owner_axis_collides_with_unique_user` warns when both are present.",
                "",
                "**Inspect**: `lazuli inspect features` adds `, owner-scope` to the resource's `(conventions: ...)` annotation and `, owner-scope` to each synth-origin command/query's `[conv:<bundle>]` tag.",
            ]
            .join("\n"),
        ),
        _ => None,
    }
}
