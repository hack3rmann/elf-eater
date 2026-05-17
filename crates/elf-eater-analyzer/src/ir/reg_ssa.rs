use crate::asm::passes::code_flow::BlockIndex;
use smallvec::SmallVec;
use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstructionSpan {
    start: u32,
    end: u32,
}

impl InstructionSpan {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn as_range(self) -> Range<usize> {
        Range {
            start: self.start as usize,
            end: self.end as usize,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Value {
    pub id: ValueId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dependency {
    pub value: ValueId,
    pub source: BlockIndex,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefinitionValue {
    #[default]
    Undefined,
    Const(u64),
    Value(ValueId),
    Add {
        left: ValueId,
        right: ValueId,
    },
    Phi {
        dependencies: SmallVec<[Dependency; 2]>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Definition {
    pub id: ValueId,
    pub value: DefinitionValue,
    pub span: InstructionSpan,
}

#[derive(Debug, Default)]
pub struct IdAllocator {
    last_id: AtomicU64,
}

impl IdAllocator {
    pub const fn new() -> Self {
        Self {
            last_id: AtomicU64::new(0),
        }
    }

    pub fn next(&self) -> ValueId {
        ValueId(self.last_id.fetch_add(1, Relaxed))
    }
}
