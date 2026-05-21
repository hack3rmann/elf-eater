use clap::Parser;
use elf_eater_analyzer::{
    asm::passes::references::{FunctionLuts, ReferencingInstruction},
    context::DisassemblerContext,
};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    path::PathBuf,
    sync::Arc,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the library to decompile
    path: PathBuf,

    /// Name of the function to build a tree for
    #[arg(short, long)]
    name: String,
}

fn walk_references(
    ctx: &DisassemblerContext,
    luts: &FunctionLuts,
    instruction_map: &mut HashMap<u64, Arc<[ReferencingInstruction]>>,
    visited: &mut HashSet<u64>,
    function_virtual_address: u64,
    depth: usize,
    func: &mut impl FnMut(&DisassemblerContext, &FunctionLuts, usize, u64, u64),
) {
    if visited.contains(&function_virtual_address) {
        return;
    }

    visited.insert(function_virtual_address);

    let instructions = Arc::clone(match instruction_map.entry(function_virtual_address) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let Some(instructions) = luts.resolve_references(ctx.elf(), function_virtual_address)
            else {
                return;
            };

            entry.insert(instructions.into())
        }
    });

    for instruction in instructions.iter() {
        if let &ReferencingInstruction::FunctionCall { virtual_address }
        | &ReferencingInstruction::PltFunctionCall {
            actual_virtual_address: virtual_address,
            plt_virtual_address: _,
        } = instruction
        {
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
    }
}

fn main() {
    let args = Args::parse();

    let ctx = DisassemblerContext::read(&args.path);
    let function_luts = FunctionLuts::new(&ctx);

    let oci_env_init_addr = function_luts.name_map[&args.name];

    walk_references(
        &ctx,
        &function_luts,
        &mut Default::default(),
        &mut Default::default(),
        oci_env_init_addr,
        0,
        &mut |ctx, _luts, depth, _parent_addr, self_addr| {
            let sym = function_luts.symbol_map[&self_addr];
            let name = sym.get_name(ctx.elf()).unwrap();

            for _ in 0..depth {
                print!(" ");
            }

            println!("{name}");
        },
    );
}
