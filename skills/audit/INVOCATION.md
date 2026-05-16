# Invoking the Lazuli Audit Skill

The skill bundle is portable — three harness paths are supported. The rule rubric (`RULES.md`) is the same across all three; only the wrapper differs.

## Path 1 — Claude Code (recommended)

The canonical path. The `SKILL.md` frontmatter at the top of this directory is read by Claude Code's skill system.

```bash
# In a Claude Code session inside a Lazuli project root:
/lazuli-audit
```

Claude reads `SKILL.md`, loads `RULES.md`, and grades the `.lzi` capsule the user provides (or the file open in the editor). The skill description triggers it automatically when the user mentions "audit my lzi" or pastes a `.lzi` block.

To install the skill globally (so it's available across all your Claude Code sessions):

```bash
# Copy or symlink:
cp -r skills/audit ~/.claude/skills/lazuli-audit
# or
ln -s "$(pwd)/skills/audit" ~/.claude/skills/lazuli-audit
```

After install, restart Claude Code. The skill is discovered via the `name: lazuli-audit` frontmatter.

## Path 2 — Cursor (rule pack)

Cursor reads project-local rules from `.cursorrules` or `.cursor/rules/*.md`. Copy `SKILL.md` and `RULES.md` into a single rule pack:

```bash
mkdir -p .cursor/rules/lazuli-audit
cp skills/audit/SKILL.md .cursor/rules/lazuli-audit/01-skill.md
cp skills/audit/RULES.md .cursor/rules/lazuli-audit/02-rules.md
cp skills/audit/LIMITATIONS.md .cursor/rules/lazuli-audit/03-limitations.md
```

Cursor surfaces the rule pack whenever a `.lzi` file is in editor context. When the user asks for an audit, the IDE-side LLM uses the loaded rules.

## Path 3 — ChatGPT / plain-prompt fallback

For ChatGPT (custom GPT or one-off chat), or any other LLM harness with no skill loader, concatenate the three files into a system prompt:

```bash
cat skills/audit/SKILL.md skills/audit/RULES.md skills/audit/LIMITATIONS.md > /tmp/lazuli-audit-prompt.md
# Then paste the contents of /tmp/lazuli-audit-prompt.md as the system prompt
# (ChatGPT: GPT Builder → Instructions; bare API: system role).
```

User-message shape:

```
Audit the following .lzi capsule against the Lazuli vocabulary catalog:

<paste .lzi content>
```

The LLM applies the loaded rubric and emits findings in the format defined in `SKILL.md` § Output format.

## Versioning

Each Lazuli release tags a corresponding skill bundle version. The bundle pins to the release:

- Lazuli `v0.4` → `skills/audit/v0.4/`
- Future versions ship as sibling directories.

When the doctor catalog evolves (new rules, deprecated rules), the bundle is regenerated for the next release. Bundles are append-only in the repo history; users on older Lazuli releases continue to use the matching skill version.

## Updating

The skill bundle is regenerated from the doctor source on each Lazuli release. **Do not edit `RULES.md` by hand** — edits are overwritten on the next regen. To propose a new rule:

1. Author a doctor lint in `crates/lazuli_doctor/src/vocab/` (per `docs/proposals/doctor-vocabulary-lints.md`).
2. Add it to `crates/lazuli_doctor/src/vocab/mod.rs`.
3. Re-run the audit-skill D.2 extraction cell (currently manual; future cell will automate via `lazuli generate skill`).

## Authority chain

`crates/lazuli_doctor/src/vocab/*.rs` (Rust source — canonical)
  ↓ projects to
`skills/audit/RULES.md` (markdown — portable mirror)
  ↓ loaded by
Claude Code / Cursor / ChatGPT (LLM consumer surface)

If the LLM emits a finding that disagrees with `lazuli doctor`, the doctor wins. Report the divergence as a skill-fidelity bug per `SKILL.md` § Authority.
