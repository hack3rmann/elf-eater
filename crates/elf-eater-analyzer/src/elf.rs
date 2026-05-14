use goblin::{Object, elf::Elf};
use std::mem;

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
