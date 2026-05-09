# Lazuli VS Code Extension

Adds Lazuli language support:

- Syntax highlighting for `.lzi` capsules
- Bracket and comment configuration

## Development

This is intentionally syntax-only for now. Open this folder in VS Code and press `F5` to run it as an Extension Development Host, or package it with `vsce` later.

No Lazuli language server is started by this extension yet.

The sample syntax follows the current capsule sketch:

```txt
feature :name do
  purpose "short product reason"
  non_goals "thing this feature does not own"

  domain do
    enum :kind, values: [:one]

    resource do
      field :name, text
    end

    constraints do
    end

    query :name do
    end

    rule "human text" do
    end

    event :name do
      field :id, ID
    end
  end

  policies do
  end

  action :name, Type do
  end

  surface do
    view :name, Type do
    end

    client :name, Type
  end

  hooks do
    server :name, Type
  end
end
```

Current consistency rules:

- Named semantic references use namespaces such as `types.customer_status`, `query.list`, `policies.update`, and `ext.status_cell`.
- Query calls use named arguments, e.g. `query.by_id(id: route.id)`.
- Predicates are represented as map values, e.g. `where: { customer.deleted_at: not_nil }`, not free-form boolean expressions.
- `<feature>.ctx.md` sits next to the capsule by convention. `context` is only an override for non-standard locations.
- Context files are source, not generated frontend/backend output.

Highlighting groups:

- Layer sections: `domain`, `surface`, `hooks`.
- Structural constructors: `feature`, `resource`, `enum`, `query`, `action`, `view`, `rule`, `event`, `escape_route`.
- Section containers: `constraints`, `policies`, `cells`.
- Internal statements: `field`, `where`, `paginate`, `requires`, `emits`, `forbid`, `source`, `columns`, and similar verbs.
