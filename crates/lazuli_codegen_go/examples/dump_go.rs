// Tiny debug binary that prints the emitted Go file. Useful for reviewing
// the codegen output during Phase J without having to wire the CLI yet.
use lazuli_codegen_go::emit_feature_go;
use lazuli_codegen_spec::customer_spike;

fn main() {
    print!("{}", emit_feature_go(&customer_spike()));
}
