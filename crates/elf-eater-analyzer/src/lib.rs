pub mod elf;

use crate::elf::OwnedElf;
use goblin::elf::{
    Elf, Sym,
    program_header::{PT_LOAD, ProgramHeader},
};
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write,
    fs, iter,
    path::Path,
    sync::Arc,
};

pub fn va_to_file_offset(va: u64, phs: &[ProgramHeader]) -> Option<usize> {
    phs.iter()
        .find(|ph| ph.p_vaddr <= va && va < ph.p_vaddr + ph.p_memsz)
        .map(|ph| (ph.p_offset + (va - ph.p_vaddr)) as usize)
}

pub struct DisassemblerContext {
    elf: Arc<OwnedElf>,
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

    pub fn elf(&self) -> &Elf<'_> {
        self.elf.data()
    }

    pub fn bytes(&self) -> &[u8] {
        self.elf.bytes()
    }

    pub fn clone_elf(&self) -> Arc<OwnedElf> {
        Arc::clone(&self.elf)
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
    FunctionCall { virtual_address: u64 },
    /// Referencing an unnamed functrion
    UnresolvedFunctionCall { virtual_address: u64 },
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
            ReferencingInstruction::UnresolvedFunctionCall { virtual_address } => {
                write!(buf, "call unresolved @ {virtual_address:X}h").unwrap();
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

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub instructions: Vec<Instruction>,
}

pub struct FunctionLuts {
    pub infos: HashMap<u64, FunctionInfo>,
    pub name_map: BTreeMap<String, u64>,
    pub symbol_map: HashMap<u64, SymExt>,
}

impl FunctionLuts {
    pub fn from_ctx(ctx: &DisassemblerContext) -> Self {
        let mut infos = HashMap::<u64, FunctionInfo>::new();
        let mut name_map = BTreeMap::<String, u64>::new();
        let mut symbol_map = HashMap::<u64, SymExt>::new();

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
            let Some(sym) = ctx.elf().dynsyms.get(reloc.r_sym) else {
                continue;
            };

            let plt_va = plt.sh_addr + 16 * (i as u64 + 1);

            symbol_map.insert(
                plt_va,
                SymExt {
                    sym,
                    ty: SymType::Dyn,
                },
            );

            if let Some(name) = ctx.elf().dynstrtab.get_at(sym.st_name) {
                name_map.insert(name.to_owned(), sym.st_value);
            }
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
                if !instruction.is_call_near() {
                    return ReferencingInstruction::Unrecognized(instruction);
                }

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
            })
            .collect()
    }
}
