pub mod elf;

use crate::elf::OwnedElf;
use goblin::elf::{
    Elf, Sym,
    program_header::{PT_LOAD, ProgramHeader},
    section_header::SHN_UNDEF,
};
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter, OpKind, Register};
use std::{collections::HashMap, fmt::Write, fs, iter, path::Path, sync::Arc};

pub fn va_to_file_offset(va: u64, phs: &[ProgramHeader]) -> Option<usize> {
    phs.iter()
        .find(|ph| ph.p_vaddr <= va && va < ph.p_vaddr + ph.p_memsz)
        .map(|ph| (ph.p_offset + (va - ph.p_vaddr)) as usize)
}

pub struct DisassemblerContext {
    pub elf: Arc<OwnedElf>,
    pub functions: Arc<[Sym]>,
    pub functions_dyn: Arc<[Sym]>,
    pub pt_loads: Arc<[ProgramHeader]>,
}

impl DisassemblerContext {
    pub fn read(path: impl AsRef<Path>) -> Self {
        // TODO(hack3rmann): handle errors
        let bytes = Box::from(fs::read(path).unwrap());
        let elf = Arc::new(OwnedElf::parse(bytes).unwrap());

        let functions = elf
            .data()
            .syms
            .iter()
            .filter(|sym| sym.is_function() && elf.data().strtab.get_at(sym.st_name).is_some())
            .collect::<Arc<[_]>>();

        let functions_dyn = elf
            .data()
            .dynsyms
            .iter()
            .filter(|sym| sym.is_function() && elf.data().dynstrtab.get_at(sym.st_name).is_some())
            .collect::<Arc<[_]>>();

        let pt_loads = elf
            .data()
            .program_headers
            .iter()
            .filter(|h| h.p_type == PT_LOAD && h.is_executable() && h.is_read())
            .cloned()
            .collect::<Arc<[_]>>();

        Self {
            elf,
            functions,
            functions_dyn,
            pt_loads,
        }
    }

    pub fn get_functions_by_name(&self, name: &str) -> impl Iterator<Item = &Sym> {
        self.functions.iter().filter_map(move |sym| {
            let this_name = self.elf.data().strtab.get_at(sym.st_name)?;
            (this_name == name).then_some(sym)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub enum FunctionSymLocation {
    #[default]
    Sym,
    DynSym,
}

#[derive(Clone, Debug)]
pub enum ReferencingInstruction {
    /// Referencing a named function with stored info
    FunctionCall {
        virtual_address: u64,
        location: FunctionSymLocation,
    },
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
            ReferencingInstruction::FunctionCall {
                virtual_address,
                location,
            } => {
                let sym = &luts.symbol_map[virtual_address];
                let name = match location {
                    FunctionSymLocation::Sym => elf.strtab.get_at(sym.st_name).unwrap(),
                    FunctionSymLocation::DynSym => elf.dynstrtab.get_at(sym.st_name).unwrap(),
                };

                write!(buf, "call {name}").unwrap();
            }
            ReferencingInstruction::Unrecognized(instruction) => {
                formatter.format(instruction, buf);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub instructions: Vec<Instruction>,
}

pub struct FunctionLuts {
    pub infos: HashMap<u64, FunctionInfo>,
    pub name_map: HashMap<String, u64>,
    pub symbol_map: HashMap<u64, Sym>,
}

impl FunctionLuts {
    pub fn from_ctx(ctx: &DisassemblerContext) -> Self {
        let mut infos = HashMap::<u64, FunctionInfo>::new();
        let mut name_map = HashMap::<String, u64>::new();
        let mut symbol_map = HashMap::<u64, Sym>::new();

        for sym in ctx.functions.iter() {
            symbol_map.insert(sym.st_value, *sym);

            if sym.st_size == 0 {
                continue;
            }

            if let Some(name) = ctx.elf.data().strtab.get_at(sym.st_name) {
                name_map.insert(name.to_owned(), sym.st_value);
            }

            let fn_start = va_to_file_offset(sym.st_value, &ctx.pt_loads).unwrap();
            let fn_end = fn_start + sym.st_size as usize;

            let mut decoder =
                Decoder::new(64, &ctx.elf.bytes()[fn_start..fn_end], DecoderOptions::NONE);
            decoder.set_ip(sym.st_value);

            let instructions = iter::from_fn(|| decoder.can_decode().then(|| decoder.decode()))
                .collect::<Vec<_>>();

            infos.insert(sym.st_value, FunctionInfo { instructions });
        }

        for sym in ctx.functions_dyn.iter() {
            symbol_map.insert(sym.st_value, *sym);

            if sym.st_size == 0 {
                continue;
            }

            if let Some(name) = ctx.elf.data().dynstrtab.get_at(sym.st_name) {
                name_map.insert(name.to_owned(), sym.st_value);
            }

            let fn_start = va_to_file_offset(sym.st_value, &ctx.pt_loads).unwrap();
            let fn_end = fn_start + sym.st_size as usize;

            let mut decoder =
                Decoder::new(64, &ctx.elf.bytes()[fn_start..fn_end], DecoderOptions::NONE);
            decoder.set_ip(sym.st_value);

            let instructions = iter::from_fn(|| decoder.can_decode().then(|| decoder.decode()))
                .collect::<Vec<_>>();

            infos.insert(sym.st_value, FunctionInfo { instructions });
        }

        Self {
            infos,
            name_map,
            symbol_map,
        }
    }

    pub fn resolve_references(
        &self,
        elf: &Elf<'_>,
        virtual_address: u64,
    ) -> Vec<ReferencingInstruction> {
        let info = &self.infos[&virtual_address];

        // TODO(hack3rmann): handle indirect (rel) calls

        info.instructions
            .iter()
            .map(|&instruction| {
                if instruction.is_call_far_indirect() || instruction.is_call_near_indirect() {
                    if instruction.op1_kind() == OpKind::Register {
                        return ReferencingInstruction::Unrecognized(instruction);
                    }

                    assert!(
                        instruction.op1_kind() == OpKind::Memory
                            && instruction.memory_base() == Register::RIP,
                        "{:?}",
                        instruction.op1_kind()
                    );
                }

                if !instruction.is_call_near() {
                    return ReferencingInstruction::Unrecognized(instruction);
                }

                let target = instruction.near_branch_target();

                let symbol = match self.symbol_map.get(&target) {
                    Some(s) => s,
                    None => return ReferencingInstruction::Unrecognized(instruction),
                };

                assert_ne!(symbol.st_value, 0);

                match (
                    elf.strtab.get_at(symbol.st_name),
                    elf.dynstrtab.get_at(symbol.st_name),
                ) {
                    (Some(_), None) | (Some(_), Some(_)) => ReferencingInstruction::FunctionCall {
                        virtual_address: target,
                        location: FunctionSymLocation::Sym,
                    },
                    (None, Some(_)) => ReferencingInstruction::FunctionCall {
                        virtual_address: target,
                        location: FunctionSymLocation::DynSym,
                    },
                    (None, None) => ReferencingInstruction::Unrecognized(instruction),
                }
            })
            .collect()
    }
}
