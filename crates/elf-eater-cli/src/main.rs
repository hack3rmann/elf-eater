use goblin::{
    Object,
    elf::program_header::{PT_LOAD, ProgramHeader},
};
use iced_x86::{Decoder, DecoderOptions, Formatter, Mnemonic, NasmFormatter};
use std::fs;

fn va_to_file_offset(va: u64, phs: &[ProgramHeader]) -> Option<usize> {
    phs.iter()
        .find(|ph| ph.p_vaddr <= va && va < ph.p_vaddr + ph.p_memsz)
        .map(|ph| (ph.p_offset + (va - ph.p_vaddr)) as usize)
}

fn main() {
    let bytes = fs::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0").unwrap();

    let Object::Elf(elf) = Object::parse(&bytes).unwrap() else {
        panic!()
    };

    let functions = elf
        .syms
        .iter()
        .filter(|sym| sym.is_function() && elf.strtab.get_at(sym.st_name).is_some());

    let pt_loads = elf
        .program_headers
        .iter()
        .filter(|h| h.p_type == PT_LOAD && h.is_executable() && h.is_read())
        .cloned()
        .collect::<Vec<_>>();

    let mut formatter = NasmFormatter::new();
    let mut fmt_buf = String::new();

    for sym in functions {
        if sym.st_size == 0 {
            continue;
        }

        let fn_start = va_to_file_offset(sym.st_value, &pt_loads).unwrap();
        let fn_end = fn_start + sym.st_size as usize;

        let mut decoder = Decoder::new(64, &bytes[fn_start..fn_end], DecoderOptions::NONE);
        decoder.set_ip(sym.st_value);

        let Some(name) = elf.strtab.get_at(sym.st_name) else {
            continue;
        };

        if name != "kokorsc" {
            continue;
        }

        println!("\n-----------------------------");
        println!("-- FUNCTION {}", name);
        println!("-----------------------------");

        while decoder.can_decode() {
            let instruction = decoder.decode();

            fmt_buf.clear();
            formatter.format(&instruction, &mut fmt_buf);

            println!("{:016x}: {}", instruction.ip(), fmt_buf);

            if let Mnemonic::Ret = instruction.mnemonic() {
                break;
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
