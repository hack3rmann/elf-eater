use elf_eater_analyzer::{DisassemblerContext, FunctionLuts};

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");
    let function_luts = FunctionLuts::from_ctx(&ctx);

    let kglssgi_sym_va = function_luts.name_map["kglssgi"];
    let kglssgi_sym = function_luts.symbol_map[&kglssgi_sym_va];

    dbg!(&function_luts.infos[&kglssgi_sym.st_value]);
}
