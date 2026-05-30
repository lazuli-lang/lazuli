/// Filesystem-resolvable view reference. Produced by the doctor
/// package from `.lzx` parses; consumed by `view_e2e_pair` calculator.
#[derive(Debug, Clone)]
pub struct LzxViewRef {
    /// `experience` name (e.g. `customer.account` or `account`).
    /// Used as the `<feature>` segment in
    /// `e2e/<feature>/<view>.spec.ts` per Wave 3.5.2 path convention.
    pub experience: String,
    /// View name (e.g. `dashboard`) — the file stem of the e2e spec.
    pub view: String,
}

/// Mutate each `LayerCoverage` to set its `verdict` based on the
/// per-layer threshold (or `"pass"` if no threshold registered).
///
/// ## Examples
///
/// ```rust
/// use std::collections::BTreeMap;
/// use lazuli_doctor::coverage::{apply_thresholds, CoverageThresholds, LayerCoverage, LayerThreshold};
///
/// let mut layers = BTreeMap::new();
/// layers.insert("handler_go".to_string(), LayerCoverage::new(40, 100));
///
/// let mut thresholds = CoverageThresholds::default();
/// thresholds.per_layer.insert(
///     "handler_go".into(),
///     LayerThreshold { block_under: 50, warn_under: 70 },
/// );
///
/// apply_thresholds(&mut layers, &thresholds);
/// assert_eq!(layers["handler_go"].verdict, "block");
/// ```
pub fn apply_thresholds(
    layers: &mut BTreeMap<String, LayerCoverage>,
    thresholds: &CoverageThresholds,
) {
    for (layer_name, layer) in layers.iter_mut() {
        let verdict = match thresholds.get(layer_name) {
            Some(t) if layer.pct < t.block_under as f64 => "block",
            Some(t) if layer.pct < t.warn_under as f64 => "warn",
            _ => "pass",
        };
        layer.verdict = verdict.to_string();
    }
}

fn compute_gate(layers: &BTreeMap<String, LayerCoverage>) -> GateResult {
    let mut below_block = Vec::new();
    let mut below_warn = Vec::new();
    for (name, layer) in layers.iter() {
        match layer.verdict.as_str() {
            "block" => below_block.push(name.clone()),
            "warn" => below_warn.push(name.clone()),
            _ => {}
        }
    }
    let verdict = if !below_block.is_empty() {
        "block"
    } else if !below_warn.is_empty() {
        "warn"
    } else {
        "pass"
    }
    .to_string();
    GateResult {
        verdict,
        below_block,
        below_warn,
    }
}

#[cfg(test)]
mod tests {
    include!("mod_tests.rs");
}
