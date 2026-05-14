use elf_eater_analyzer::{DisassemblerContext, FunctionLuts};
use iced_x86::NasmFormatter;
use std::collections::BTreeMap;

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");
    let function_luts = FunctionLuts::from_ctx(&ctx);

    let instructions = function_luts
        .name_map
        .iter()
        .map(|(name, &sym_va)| {
            (
                name.to_owned(),
                function_luts.resolve_references(ctx.elf(), sym_va),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let name = "nigsui";
    let instructions = &instructions[name];

    let mut formatter = NasmFormatter::new();
    let mut buf = String::new();

    println!("\n\nFUNCTION {name}\n");

    for instruction in instructions.iter() {
        buf.clear();
        instruction.format(&function_luts, ctx.elf(), &mut formatter, &mut buf);

        println!("{buf}");
    }
}
