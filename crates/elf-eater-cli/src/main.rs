use elf_eater_analyzer::{DisassemblerContext, FunctionLuts, ReferencingInstruction};
use iced_x86::NasmFormatter;

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");
    let function_luts = FunctionLuts::from_ctx(&ctx);

    for (name, &sym_va) in &function_luts.name_map {
        let sym = function_luts.symbol_map[&sym_va];

        let instructions = function_luts.resolve_references(ctx.elf(), sym.sym.st_value);
        let mut formatter = NasmFormatter::new();
        let mut buf = String::new();

        if !instructions
            .iter()
            .any(|i| matches!(i, ReferencingInstruction::FunctionCall { .. }))
        {
            continue;
        }

        println!("\n\nFUNCTION {name}\n");

        for instruction in instructions.iter() {
            buf.clear();
            instruction.format(&function_luts, ctx.elf(), &mut formatter, &mut buf);

            println!("{buf}");
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
