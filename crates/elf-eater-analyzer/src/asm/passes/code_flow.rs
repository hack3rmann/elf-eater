use crate::asm::passes::{
    references::{FunctionInfo, FunctionLuts},
    semantic::SemanticInstruction,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodeBlockTerminator {
    Return,
    InternalJump {
        block_index: BlockIndex,
        address: u64,
    },
    IndirectJump,
    ExternalJump {
        address: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CodeBlock {
    pub instruction_slice: Range<u32>,
    pub terminator: Option<CodeBlockTerminator>,
    pub fallthrough_to: Option<BlockIndex>,
}

impl CodeBlock {
    pub fn range(&self) -> Range<usize> {
        Range {
            start: self.instruction_slice.start as usize,
            end: self.instruction_slice.end as usize,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockIndex(pub u32);

impl BlockIndex {
    const INVALID: Self = Self(u32::MAX);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionCodeFlow {
    pub blocks: Vec<CodeBlock>,
    pub index_to_block: HashMap<u32, BlockIndex>,
}

impl FunctionCodeFlow {
    pub fn new(luts: &FunctionLuts, fn_address: u64) -> Self {
        let sym = luts.symbol_map[&fn_address];
        let function_end = fn_address + sym.sym.st_size;
        let info = &luts.infos[&fn_address];

        let mut referenced_instructions = HashSet::<u32>::new();

        for &instruction in &info.instructions {
            match instruction {
                SemanticInstruction::ConditionalJump { address, ty: _ }
                | SemanticInstruction::DirectJump { address } => {
                    if fn_address <= address && address < function_end {
                        let ref_index = info.address_map[&address];
                        referenced_instructions.insert(ref_index as u32);
                    }
                }
                _ => {}
            }
        }

        let mut block_start = 0_u32;
        let mut blocks = Vec::new();
        let mut index_to_block = HashMap::<u32, BlockIndex>::new();

        for (&instruction, i) in info.instructions.iter().zip(0_u32..) {
            if referenced_instructions.contains(&i) {
                if block_start != i {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockIndex(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i,
                        terminator: None,
                        fallthrough_to: Some(BlockIndex(i)),
                    });
                }

                block_start = i;
            }

            match instruction {
                SemanticInstruction::Return | SemanticInstruction::ReturnClear { amount: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockIndex(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(CodeBlockTerminator::Return),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::IndirectJumpReg { register: _ }
                | SemanticInstruction::IndirectJumpMem { expr: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockIndex(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(CodeBlockTerminator::IndirectJump),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::DirectJump { address } => {
                    let term = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalJump {
                            block_index: BlockIndex::INVALID,
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalJump { address }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockIndex(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(term),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::ConditionalJump { address, ty: _ } => {
                    let term = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalJump {
                            block_index: BlockIndex::INVALID,
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalJump { address }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockIndex(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(term),
                        // Store instruction's index temporary
                        fallthrough_to: Some(BlockIndex(i + 1)),
                    });
                }
                _ => continue,
            }

            block_start = i + 1;
        }

        for block in &mut blocks {
            if let Some(CodeBlockTerminator::InternalJump {
                block_index,
                address,
                ..
            }) = &mut block.terminator
            {
                let index = info.address_map[address];
                *block_index = index_to_block[&(index as u32)];
            }

            if let Some(to) = &mut block.fallthrough_to {
                *to = index_to_block[&to.0];
            }
        }

        Self {
            blocks,
            index_to_block,
        }
    }

    pub fn address_to_index(&self, info: &FunctionInfo, address: u64) -> Option<BlockIndex> {
        let instruction_index = *info.address_map.get(&address)? as u32;
        self.index_to_block.get(&instruction_index).copied()
    }
}
