#[derive(Debug)]
struct PendingTool {
    name: String,
    line: usize,
    effect: Option<ToolEffect>,
    effect_invalid: bool,
    pii_classes: Vec<QualifiedName>,
    adapter: Option<QualifiedName>,
}

fn flush_pending_tool(
    pending: &mut Option<PendingTool>,
    registry: &mut AppRegistry,
    defects: &mut Vec<RegistryToolEntryDefect>,
) {
    let Some(tool) = pending.take() else { return };

    if tool.effect_invalid {
        defects.push(RegistryToolEntryDefect {
            line: tool.line,
            name: tool.name,
            reason: RegistryToolDefectReason::EffectInvalid,
        });
        return;
    }

    let Some(effect) = tool.effect else {
        defects.push(RegistryToolEntryDefect {
            line: tool.line,
            name: tool.name,
            reason: RegistryToolDefectReason::EffectMissing,
        });
        return;
    };

    registry.tools.push(RegistryToolEntry {
        name: tool.name,
        effect,
        pii_classes: tool.pii_classes,
        adapter: tool.adapter,
        span_ref: None,
    });
}

/// Normalise a raw `pii_classes` token (e.g. `contact`, `@pii.contact`)
/// to the canonical closed-namespace form. The IR keeps it as a string
/// inside `QualifiedName::name` so doctor can compare against the
/// agent-side `@pii.*` references uniformly.
fn pii_class_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("@pii.") {
        trimmed.to_owned()
    } else {
        format!("@pii.{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_registry_header_yields_default() {
        let out = parse_app_registry_with_defects("# nothing here\n");
        assert!(out.registry.is_none());
        assert!(out.tool_defects.is_empty());
    }

    #[test]
    fn tool_without_effect_is_defective() {
        let src = "registry\n  tools\n    tool stale\n";
        let out = parse_app_registry_with_defects(src);
        let registry = out.registry.expect("registry");
        assert!(registry.tools.is_empty());
        assert_eq!(out.tool_defects.len(), 1);
        assert_eq!(out.tool_defects[0].name, "stale");
    }
}
