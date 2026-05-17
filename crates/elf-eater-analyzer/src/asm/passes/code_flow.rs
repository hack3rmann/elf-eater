use crate::asm::passes::{
    references::{FunctionInfo, FunctionLuts},
    semantic::SemanticInstruction,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodeBlockTerminator {
    #[default]
    Return,
    InternalJump {
        target: BlockId,
        address: u64,
    },
    ExternalJump {
        address: u64,
    },
    InternalBranch {
        jump_to: BlockId,
        fallthrough: BlockId,
        address: u64,
    },
    ExternalBranch {
        fallthrough: BlockId,
        address: u64,
    },
    Fallthrough {
        target: BlockId,
    },
    IndirectJump,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CodeBlock {
    pub instruction_slice: Range<u32>,
    pub terminator: CodeBlockTerminator,
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
pub struct BlockId(pub u32);

impl BlockId {
    const INVALID: Self = Self(u32::MAX);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionCodeFlow {
    pub blocks: Vec<CodeBlock>,
    pub index_to_block: HashMap<u32, BlockId>,
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
        let mut index_to_block = HashMap::<u32, BlockId>::new();

        for (&instruction, i) in info.instructions.iter().zip(0_u32..) {
            if referenced_instructions.contains(&i) {
                if block_start != i {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockId(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i,
                        terminator: CodeBlockTerminator::Fallthrough { target: BlockId(i) },
                    });
                }

                block_start = i;
            }

            match instruction {
                SemanticInstruction::Return | SemanticInstruction::ReturnClear { amount: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockId(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: CodeBlockTerminator::Return,
                    });
                }
                SemanticInstruction::IndirectJumpReg { register: _ }
                | SemanticInstruction::IndirectJumpMem { expr: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockId(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: CodeBlockTerminator::IndirectJump,
                    });
                }
                SemanticInstruction::DirectJump { address } => {
                    let terminator = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalJump {
                            target: BlockId::INVALID,
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalJump { address }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockId(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator,
                    });
                }
                SemanticInstruction::ConditionalJump { address, ty: _ } => {
                    let terminator = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalBranch {
                            // Store instruction's index temporary
                            jump_to: BlockId::INVALID,
                            fallthrough: BlockId(i + 1),
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalBranch {
                            // Store instruction's index temporary
                            fallthrough: BlockId(i + 1),
                            address,
                        }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, BlockId(block_index));

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator,
                    });
                }
                _ => continue,
            }

            block_start = i + 1;
        }

        for block in &mut blocks {
            if let CodeBlockTerminator::InternalJump {
                target, address, ..
            }
            | CodeBlockTerminator::InternalBranch {
                jump_to: target,
                fallthrough: _,
                address,
            } = &mut block.terminator
            {
                let index = info.address_map[address];
                *target = index_to_block[&(index as u32)];
            }

            if let CodeBlockTerminator::InternalBranch { fallthrough, .. }
            | CodeBlockTerminator::ExternalBranch { fallthrough, .. }
            | CodeBlockTerminator::Fallthrough {
                target: fallthrough,
            } = &mut block.terminator
            {
                *fallthrough = index_to_block[&fallthrough.0];
            }
        }

        Self {
            blocks,
            index_to_block,
        }
    }

    pub fn address_to_index(&self, info: &FunctionInfo, address: u64) -> Option<BlockId> {
        let instruction_index = *info.address_map.get(&address)? as u32;
        self.index_to_block.get(&instruction_index).copied()
    }
}
