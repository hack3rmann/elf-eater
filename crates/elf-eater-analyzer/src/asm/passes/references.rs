use crate::{asm::va_to_file_offset, context::DisassemblerContext};
use goblin::elf::{Elf, Reloc, Sym};
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter, OpKind, Register};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write,
    iter,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub enum FunctionSymLocation {
    #[default]
    Sym,
    DynSym,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReferencingInstruction {
    /// Referencing a named function with stored info
    FunctionCall { virtual_address: u64 },
    /// Referencing a named function with stored info and calling it via PLT stub
    PltFunctionCall {
        actual_virtual_address: u64,
        plt_virtual_address: u64,
    },
    /// Referencing an unnamed functrion
    UnresolvedFunctionCall { virtual_address: u64 },
    /// Referencing an unnamed function via plt
    UnresolvedPltFunctionCall { plt_virtual_address: u64 },
    /// Other
    Unrecognized(Instruction),
}

impl ReferencingInstruction {
    pub fn format(
        &self,
        luts: &FunctionLuts,
        elf: &Elf<'_>,
        formatter: &mut NasmFormatter,
        buf: &mut String,
    ) {
        match self {
            ReferencingInstruction::FunctionCall { virtual_address } => {
                let sym = &luts.symbol_map[virtual_address];

                let name = match sym.ty {
                    SymType::Regular => elf.strtab.get_at(sym.sym.st_name).unwrap(),
                    SymType::Dyn => elf.dynstrtab.get_at(sym.sym.st_name).unwrap(),
                };

                write!(buf, "call {name}").unwrap();
            }
            ReferencingInstruction::PltFunctionCall {
                actual_virtual_address,
                plt_virtual_address,
            } => {
                let sym = &luts.symbol_map[actual_virtual_address];

                let name = match sym.ty {
                    SymType::Regular => elf.strtab.get_at(sym.sym.st_name).unwrap(),
                    SymType::Dyn => elf.dynstrtab.get_at(sym.sym.st_name).unwrap(),
                };

                write!(buf, "call {name} @ plt[{plt_virtual_address:X}h]").unwrap();
            }
            ReferencingInstruction::UnresolvedFunctionCall { virtual_address } => {
                write!(buf, "call unresolved @ {virtual_address:X}h").unwrap();
            }
            ReferencingInstruction::UnresolvedPltFunctionCall {
                plt_virtual_address,
            } => {
                write!(buf, "call unresolved @ plt[{plt_virtual_address:X}h]").unwrap();
            }
            ReferencingInstruction::Unrecognized(instruction) => {
                formatter.format(instruction, buf);
            }
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymType {
    #[default]
    Regular,
    Dyn,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct SymExt {
    pub sym: Sym,
    pub ty: SymType,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FunctionInfo {
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FunctionLuts {
    pub infos: HashMap<u64, FunctionInfo>,
    pub name_map: BTreeMap<String, u64>,
    pub dyn_name_map: BTreeMap<String, u64>,
    pub got_to_reloc: HashMap<u64, Reloc>,
    pub symbol_map: HashMap<u64, SymExt>,
}

impl FunctionLuts {
    pub fn new(ctx: &DisassemblerContext) -> Self {
        let mut infos = HashMap::<u64, FunctionInfo>::new();
        let mut name_map = BTreeMap::<String, u64>::new();
        let mut dyn_name_map = BTreeMap::<String, u64>::new();
        let mut symbol_map = HashMap::<u64, SymExt>::new();
        let mut got_to_reloc = HashMap::<u64, Reloc>::new();

        for sym in ctx.functions.iter() {
            symbol_map.insert(
                sym.st_value,
                SymExt {
                    sym: *sym,
                    ty: SymType::Regular,
                },
            );

            if sym.st_size == 0 {
                continue;
            }

            if let Some(name) = ctx.elf().strtab.get_at(sym.st_name) {
                name_map.insert(name.to_owned(), sym.st_value);
            }

            let fn_start = va_to_file_offset(sym.st_value, &ctx.pt_loads).unwrap();
            let fn_end = fn_start + sym.st_size as usize;

            let mut decoder =
                Decoder::new(64, &ctx.bytes()[fn_start..fn_end], DecoderOptions::NONE);
            decoder.set_ip(sym.st_value);

            let instructions = iter::from_fn(|| decoder.can_decode().then(|| decoder.decode()))
                .collect::<Vec<_>>();

            infos.insert(sym.st_value, FunctionInfo { instructions });
        }

        let plt = ctx
            .elf()
            .section_headers
            .iter()
            .find(|header| ".plt" == ctx.elf().shdr_strtab.get_at(header.sh_name).unwrap())
            .unwrap();

        for (i, reloc) in ctx.elf().pltrelocs.iter().enumerate() {
            got_to_reloc.insert(reloc.r_offset, reloc);

            let Some(sym) = ctx.elf().dynsyms.get(reloc.r_sym) else {
                continue;
            };

            // FIXME(hack3rmann): could be 32-byte offset actually
            let plt_va = plt.sh_addr + 16 * (i as u64 + 1);

            symbol_map.insert(
                plt_va,
                SymExt {
                    sym,
                    ty: SymType::Dyn,
                },
            );

            if let Some(name) = ctx.elf().dynstrtab.get_at(sym.st_name) {
                dyn_name_map.insert(name.to_owned(), plt_va);
            }

            let fn_start = va_to_file_offset(plt_va, &ctx.pt_loads).unwrap();
            let fn_end = fn_start + 16;

            let mut decoder =
                Decoder::new(64, &ctx.bytes()[fn_start..fn_end], DecoderOptions::NONE);
            decoder.set_ip(plt_va);

            let instructions = iter::from_fn(|| decoder.can_decode().then(|| decoder.decode()))
                .collect::<Vec<_>>();

            infos.insert(plt_va, FunctionInfo { instructions });
        }

        Self {
            infos,
            name_map,
            dyn_name_map,
            symbol_map,
            got_to_reloc,
        }
    }

    fn rewrite_regular_call_instruction(
        &self,
        elf: &Elf<'_>,
        instruction: Instruction,
    ) -> ReferencingInstruction {
        let virtual_address = instruction.near_branch_target();

        let symbol = match self.symbol_map.get(&virtual_address) {
            Some(s) => s,
            None => return ReferencingInstruction::Unrecognized(instruction),
        };

        if let SymType::Regular = symbol.ty
            && elf.strtab.get_at(symbol.sym.st_name).is_some()
        {
            ReferencingInstruction::FunctionCall { virtual_address }
        } else if let SymType::Dyn = symbol.ty
            && elf.dynstrtab.get_at(symbol.sym.st_name).is_some()
        {
            ReferencingInstruction::FunctionCall { virtual_address }
        } else {
            ReferencingInstruction::UnresolvedFunctionCall { virtual_address }
        }
    }

    fn rewrite_plt_call_instruction(
        &self,
        elf: &Elf<'_>,
        instruction: Instruction,
        first_instruction: Instruction,
    ) -> ReferencingInstruction {
        let call_address = instruction.near_branch_target();
        let unresolved = ReferencingInstruction::UnresolvedPltFunctionCall {
            plt_virtual_address: call_address,
        };

        let got_address = first_instruction.ip_rel_memory_address();

        let Some(reloc) = self.got_to_reloc.get(&got_address) else {
            return unresolved;
        };

        let Some(sym) = elf.dynsyms.get(reloc.r_sym) else {
            return unresolved;
        };

        ReferencingInstruction::PltFunctionCall {
            actual_virtual_address: sym.st_value,
            plt_virtual_address: call_address,
        }
    }

    fn probably_plt_call(&self, instruction: Instruction) -> Option<Instruction> {
        if !instruction.is_call_near() {
            return None;
        }

        let virtual_address = instruction.near_branch_target();
        let info = self.infos.get(&virtual_address)?;
        let first_instr = info.instructions.first()?;

        // jmp qword [rel 0xWHATEVER]
        (first_instr.is_jmp_near_indirect()
            && first_instr.op0_kind() == OpKind::Memory
            && first_instr.memory_base() == Register::RIP)
            .then_some(*first_instr)
    }

    pub fn resolve_references(
        &self,
        elf: &Elf<'_>,
        virtual_address: u64,
    ) -> Option<Vec<ReferencingInstruction>> {
        let info = &self.infos.get(&virtual_address)?;

        let instructions = info
            .instructions
            .iter()
            .map(|&instruction| {
                if !instruction.is_call_near() {
                    return ReferencingInstruction::Unrecognized(instruction);
                }

                if let Some(first_instruction) = self.probably_plt_call(instruction) {
                    self.rewrite_plt_call_instruction(elf, instruction, first_instruction)
                } else {
                    self.rewrite_regular_call_instruction(elf, instruction)
                }
            })
            .collect();

        Some(instructions)
    }
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct ReferencePassResult {
    pub luts: FunctionLuts,
    pub instruction_map: HashMap<u64, Vec<ReferencingInstruction>>,
}

impl ReferencePassResult {
    pub fn new(ctx: &DisassemblerContext) -> Self {
        let luts = FunctionLuts::new(ctx);

        let instruction_map = luts
            .name_map
            .values()
            .flat_map(|&virtual_address| {
                let instructions = luts.resolve_references(ctx.elf(), virtual_address)?;
                Some((virtual_address, instructions))
            })
            .collect();

        Self {
            luts,
            instruction_map,
        }
    }
}
