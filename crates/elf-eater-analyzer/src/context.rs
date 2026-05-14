use crate::elf::OwnedElf;
use goblin::elf::{Elf, ProgramHeader, Sym, program_header::PT_LOAD};
use std::{fs, path::Path, sync::Arc};

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
