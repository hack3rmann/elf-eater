use elf_eater_analyzer::{
    asm::passes::references::{FunctionLuts, ReferencingInstruction, SymType},
    context::DisassemblerContext,
};
use std::collections::{HashMap, HashSet, hash_map::Entry};

fn walk_references(
    ctx: &DisassemblerContext,
    luts: &FunctionLuts,
    instruction_map: &mut HashMap<u64, Vec<ReferencingInstruction>>,
    visited: &mut HashSet<u64>,
    function_virtual_address: u64,
    depth: usize,
    func: &mut impl FnMut(&DisassemblerContext, &FunctionLuts, usize, u64, u64),
) {
    if visited.contains(&function_virtual_address) {
        return;
    }

    visited.insert(function_virtual_address);

    let instructions = match instruction_map.entry(function_virtual_address) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let Some(instructions) = luts.resolve_references(ctx.elf(), function_virtual_address)
            else {
                return;
            };

            entry.insert(instructions)
        }
    };

    for instruction in instructions.clone() {
        match instruction {
            ReferencingInstruction::FunctionCall { virtual_address } => {
                if visited.contains(&virtual_address) {
                    continue;
                }

                func(ctx, luts, depth, function_virtual_address, virtual_address);

                walk_references(
                    ctx,
                    luts,
                    instruction_map,
                    visited,
                    virtual_address,
                    depth + 1,
                    func,
                );
            }
            ReferencingInstruction::PltFunctionCall {
                actual_virtual_address,
                plt_virtual_address: _,
            } => {
                if visited.contains(&actual_virtual_address) {
                    continue;
                }

                func(
                    ctx,
                    luts,
                    depth,
                    function_virtual_address,
                    actual_virtual_address,
                );

                walk_references(
                    ctx,
                    luts,
                    instruction_map,
                    visited,
                    actual_virtual_address,
                    depth + 1,
                    func,
                );
            }
            _ => {}
        }
    }
}

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");
    let function_luts = FunctionLuts::new(&ctx);

    let name = "kgumini";
    let sym_va = function_luts.name_map[name];

    let mut visited = HashSet::new();

    walk_references(
        &ctx,
        &function_luts,
        &mut HashMap::new(),
        &mut visited,
        sym_va,
        0,
        &mut |ctx, _luts, depth, _parent_address, address| {
            let sym = function_luts.symbol_map[&address];
            let name = match sym.ty {
                SymType::Regular => ctx.elf().strtab.get_at(sym.sym.st_name).unwrap(),
                SymType::Dyn => ctx.elf().dynstrtab.get_at(sym.sym.st_name).unwrap(),
            };

            for _ in 0..depth {
                print!(" ");
            }

            println!("{name}");
        },
    );

    dbg!(visited.len());
}
