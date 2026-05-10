use lazuli_codegen_spec::customer_spike;
use lazuli_codegen_ts::emit_feature_ts;

fn main() {
    print!("{}", emit_feature_ts(&customer_spike()));
}
