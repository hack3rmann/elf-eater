use goblin::{
    Object,
    elf::{
        Elf, Sym,
        program_header::{PT_LOAD, ProgramHeader},
    },
};
use iced_x86::{Decoder, DecoderOptions, Formatter, NasmFormatter};
use std::{fs, mem, path::Path, sync::Arc};

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
}

fn main() {
    let ctx = DisassemblerContext::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0");

    let mut formatter = NasmFormatter::new();
    let mut fmt_buf = String::new();

    for sym in ctx.functions.iter() {
        if sym.st_size == 0 {
            continue;
        }

        let fn_start = va_to_file_offset(sym.st_value, &ctx.pt_loads).unwrap();
        let fn_end = fn_start + sym.st_size as usize;

        let mut decoder =
            Decoder::new(64, &ctx.elf.bytes()[fn_start..fn_end], DecoderOptions::NONE);
        decoder.set_ip(sym.st_value);

        let Some(name) = ctx.elf.data().strtab.get_at(sym.st_name) else {
            continue;
        };

        println!("\n-----------------------------");
        println!("-- FUNCTION {}", name);
        println!("-----------------------------");

        while decoder.can_decode() {
            let instruction = decoder.decode();

            fmt_buf.clear();
            formatter.format(&instruction, &mut fmt_buf);

            print!("{:016x}: ", instruction.ip());

            let mut is_substituted = false;

            if instruction.is_call_near() {
                let target = instruction.near_branch_target();

                for sym in ctx.functions.iter() {
                    if sym.st_value.abs_diff(target) > 4 {
                        continue;
                    }

                    let Some(name) = ctx.elf.data().strtab.get_at(sym.st_name) else {
                        continue;
                    };

                    println!("call {name}");

                    is_substituted = true;
                    break;
                }
            }

            if !is_substituted {
                println!("{fmt_buf}");
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
