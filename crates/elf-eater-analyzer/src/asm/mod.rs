pub mod passes;

use goblin::elf::program_header::ProgramHeader;
use iced_x86::{Formatter, Instruction, NasmFormatter};

#[allow(unused)]
pub(crate) fn dbg_fmt_instruction(instruction: &Instruction) -> String {
    let mut buf = String::new();
    let mut formatter = NasmFormatter::new();
    formatter.format(instruction, &mut buf);
    buf
}

pub fn va_to_file_offset(va: u64, phs: &[ProgramHeader]) -> Option<usize> {
    phs.iter()
        .find(|ph| ph.p_vaddr <= va && va < ph.p_vaddr + ph.p_memsz)
        .map(|ph| (ph.p_offset + (va - ph.p_vaddr)) as usize)
}
