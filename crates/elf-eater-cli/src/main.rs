use goblin::{
    Object,
    elf::{
        Elf, Sym,
        program_header::{PT_LOAD, ProgramHeader},
    },
};
use iced_x86::{Decoder, DecoderOptions, Instruction};
use std::{collections::HashMap, fs, iter, mem, path::Path, sync::Arc};

fn va_to_file_offset(va: u64, phs: &[ProgramHeader]) -> Option<usize> {
    phs.iter()
        .find(|ph| ph.p_vaddr <= va && va < ph.p_vaddr + ph.p_memsz)
        .map(|ph| (ph.p_offset + (va - ph.p_vaddr)) as usize)
}

#[derive(Debug)]
pub struct OwnedElf {
    // NOTE(hack3rmann): elf must be
    // 1. dropped before bytes get dropped
    // 2. private so no one can copy a reference with lifetime `'static`
    elf: Elf<'static>,
    // NOTE(hack3rmann): bytes must be immutable
    bytes: Box<[u8]>,
}

impl OwnedElf {
    pub fn parse(bytes: Box<[u8]>) -> Result<Self, goblin::error::Error> {
        let Object::Elf(elf) = Object::parse(&bytes)? else {
            // TODO: handle the error
            panic!()
        };

        Ok(Self {
            // Safety: bytes will live at least for the lifetime of the elf
            elf: unsafe { mem::transmute::<Elf, Elf<'static>>(elf) },
            bytes,
        })
    }

    pub fn data(&self) -> &Elf<'_> {
        // NOTE(hack3rmann): `Elf` must capture the lifetime of &self too, therefore no `Deref`
        // impl is allowed
        &self.elf
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

// NOTE(hack3rmann): empty Drop impl so no one can move out of self.bytes
impl Drop for OwnedElf {
    fn drop(&mut self) {}
}

pub struct DisassemblerContext {
    pub elf: Arc<OwnedElf>,
    pub functions: Arc<[Sym]>,
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

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub instructions: Vec<Instruction>,
}

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");

    let mut functions = HashMap::<u64, FunctionInfo>::new();
    let mut function_name_map = HashMap::<String, u64>::new();
    let mut symbol_map = HashMap::<u64, Sym>::new();

    for sym in ctx.functions.iter() {
        symbol_map.insert(sym.st_value, *sym);

        if sym.st_size == 0 {
            continue;
        }

        if let Some(name) = ctx.elf.data().strtab.get_at(sym.st_name) {
            function_name_map.insert(name.to_owned(), sym.st_value);
        }

        let fn_start = va_to_file_offset(sym.st_value, &ctx.pt_loads).unwrap();
        let fn_end = fn_start + sym.st_size as usize;

        let mut decoder =
            Decoder::new(64, &ctx.elf.bytes()[fn_start..fn_end], DecoderOptions::NONE);
        decoder.set_ip(sym.st_value);

        let instructions =
            iter::from_fn(|| decoder.can_decode().then(|| decoder.decode())).collect::<Vec<_>>();

        functions.insert(sym.st_value, FunctionInfo { instructions });
    }

    let kglssgi_sym_va = function_name_map["kglssgi"];
    let kglssgi_sym = symbol_map[&kglssgi_sym_va];

    dbg!(&functions[&kglssgi_sym.st_value]);
}
