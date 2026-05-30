    use crate::{AnalyzeError, lower_surface};
    use lazuli_ir as ir;
    use lazuli_syntax::parse_surface_document;

    fn parse(src: &str) -> ir::Surface {
        let ast = parse_surface_document(src).expect("parses");
        lower_surface(&ast).expect("lowers")
    }

    fn parse_requires(atom: &str) -> ir::PolicyAtom {
        let source = format!("surface slug web\n  audience admin\n    requires {atom}\n");
        let surface = parse(&source);
        surface.audiences[0].requires[0].clone()
    }

include!("surface_lowering_p1_tests.rs");
include!("surface_lowering_p2_tests.rs");
