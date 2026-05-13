use goblin::Object;
use iced_x86::{Decoder, DecoderOptions, Formatter, Mnemonic, NasmFormatter};
use std::fs;

fn main() {
    let buffer = fs::read("/home/hack3rmann/Downloads/libclntsh.so.12.1.0").unwrap();

    let Object::Elf(elf) = Object::parse(&buffer).unwrap() else {
        panic!()
    };

    let functions = elf
        .syms
        .iter()
        .filter(|sym| sym.is_function() && elf.strtab.get_at(sym.st_name).is_some());

    let text_section_header = elf
        .section_headers
        .iter()
        .find_map(|header| {
            let name = elf.shdr_strtab.get_at(header.sh_name)?;
            (name == ".text").then_some(header)
        })
        .expect("there's no `.text` section");

    let text_start = text_section_header.sh_offset as usize;
    let text_len = text_section_header.sh_size as usize;
    let text_end = text_start + text_len;

    let text_section_bytes = &buffer[text_start..text_end];

    let mut decoder = Decoder::new(64, text_section_bytes, DecoderOptions::NONE);
    decoder.set_ip(text_section_header.sh_addr);

    let mut formatter = NasmFormatter::new();
    let mut fmt_buf = String::new();

    for sym in functions {
        if (sym.st_value as usize) < text_start || (sym.st_value as usize) >= text_end {
            continue;
        }

        let offset = match sym.st_value.checked_sub(text_section_header.sh_addr) {
            Some(v) => v as usize,
            None => continue,
        };

        decoder.set_position(offset).unwrap();

        let Some(name) = elf.strtab.get_at(sym.st_name) else {
            continue;
        };

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
