    // Package fixture helpers extracted from .
    // Tightly coupled to  internals — kept as 
    // re-exports via the parent's .

    use std::path::PathBuf;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;

    use lazuli_analyzer::lower_feature_skeleton;
    use lazuli_syntax::{self, parse_feature_skeletons};

    use crate::doctor::*;

include!("test_support_packages_p1.rs");
include!("test_support_packages_p2.rs");
