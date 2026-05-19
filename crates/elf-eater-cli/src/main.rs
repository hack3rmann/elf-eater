use elf_eater_analyzer::{
    asm::passes::{
        code_flow::{BlockId, CodeBlockTerminator, FunctionCodeFlow},
        references::{FunctionLuts, ReferencingInstruction},
        semantic::{ArithmeticInstruction, ArithmeticOpKind, SemanticInstruction},
    },
    context::DisassemblerContext,
    ir::reg_ssa::Ssa,
};
use iced_x86::{Mnemonic, NasmFormatter};
use petgraph::{algo::dominators, graph::NodeIndex};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::Arc,
};

#[allow(unused)]
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

fn _find_instruction(luts: &FunctionLuts) -> Option<&str> {
    for (name, &address) in &luts.name_map {
        let Some(info) = luts.infos.get(&address) else {
            continue;
        };

        let div = info.instructions.iter().find(|instr| match instr {
            SemanticInstruction::Arithmetic(ArithmeticInstruction {
                kind: ArithmeticOpKind::Idiv,
                ..
            }) => true,
            SemanticInstruction::Other(instruction) => {
                matches!(instruction.mnemonic(), Mnemonic::Idiv)
            }
            _ => false,
        });

        if div.is_some() {
            return Some(name);
        }
    }

    None
}

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");
    let function_luts = FunctionLuts::new(&ctx);

    // let name = "kgumini";
    // let name = "qctdccso";
    let name = "kguudltr";
    // let name = "Java_oracle_streams_XStreamIn_XStreamInAttachNative"; // has div
    // let name = "kghfnd"; // has imul
    // let name = "dbgtbUpdateBucketUtil"; // has idiv
    let sym_va = function_luts.name_map[name];
    let info = &function_luts.infos[&sym_va];

    let mut formatter = NasmFormatter::new();
    let mut buf = String::new();

    let code_flow = FunctionCodeFlow::new(&function_luts, sym_va);

    println!("fn {name} {{");

    for (i, block) in code_flow.blocks.iter().enumerate() {
        println!("    block_{i} {{");

        for instruction in &info.instructions[block.span.range()] {
            buf.clear();
            instruction.format(&mut formatter, &mut buf);

            println!("        {buf}");
        }

        match block.terminator {
            CodeBlockTerminator::InternalJump {
                target: BlockId(target),
                address: _,
            }
            | CodeBlockTerminator::Fallthrough {
                target: BlockId(target),
            } => {
                println!("        ; goto block_{target}")
            }
            CodeBlockTerminator::InternalBranch {
                jump_to: BlockId(jump_to),
                fallthrough: BlockId(fallthrough),
                address: _,
            } => println!("        ; branch block_{jump_to}, block_{fallthrough}"),
            _ => {}
        }

        println!("    }}");
    }

    println!("}}");

    let cfg = code_flow.build_cfg();
    let dominators = dominators::simple_fast(&cfg, NodeIndex::new(0));

    let _ssa = Ssa::build(&code_flow, info);

    dbg!(dominators);
}
