// Dumps the customer_spike fixture as pretty JSON. Used to seed
// `examples/runtime-spec/customer.json` and to regenerate it whenever the
// hand-built spike changes shape.
use lazuli_codegen_spec::customer_spike;

fn main() {
    let feature = customer_spike();
    let json = serde_json::to_string_pretty(&feature).expect("serialize");
    println!("{json}");
}
