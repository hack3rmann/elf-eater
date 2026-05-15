use crate::asm::passes::{references::FunctionLuts, semantic::SemanticInstruction};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodeBlockTerminator {
    Return {
        instruction_index: u32,
    },
    InternalJump {
        instruction_index: u32,
        block_index: u32,
        address: u64,
    },
    IndirectJump {
        instruction_index: u32,
    },
    ExternalJump {
        instruction_index: u32,
        address: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CodeBlock {
    pub instruction_slice: Range<u32>,
    pub terminator: Option<CodeBlockTerminator>,
    pub fallthrough_to: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FunctionCodeFlow {
    pub blocks: Vec<CodeBlock>,
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

        const INVALID_BLOCK: u32 = u32::MAX;

        let mut block_start = 0_u32;
        let mut blocks = Vec::new();
        let mut index_to_block = HashMap::<u32, u32>::new();

        for (&instruction, i) in info.instructions.iter().zip(0_u32..) {
            if referenced_instructions.contains(&i) {
                if block_start != i {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, block_index);

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i,
                        terminator: None,
                        fallthrough_to: Some(i),
                    });
                }

                block_start = i;
            }

            match instruction {
                SemanticInstruction::Return | SemanticInstruction::ReturnClear { amount: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, block_index);

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(CodeBlockTerminator::Return {
                            instruction_index: i,
                        }),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::IndirectJumpReg { register: _ }
                | SemanticInstruction::IndirectJumpMem { expr: _ } => {
                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, block_index);

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(CodeBlockTerminator::IndirectJump {
                            instruction_index: i,
                        }),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::DirectJump { address } => {
                    let term = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalJump {
                            instruction_index: i,
                            block_index: INVALID_BLOCK,
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalJump {
                            instruction_index: i,
                            address,
                        }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, block_index);

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(term),
                        fallthrough_to: None,
                    });
                }
                SemanticInstruction::ConditionalJump { address, ty: _ } => {
                    let term = if fn_address <= address && address < function_end {
                        CodeBlockTerminator::InternalJump {
                            instruction_index: i,
                            block_index: INVALID_BLOCK,
                            address,
                        }
                    } else {
                        CodeBlockTerminator::ExternalJump {
                            instruction_index: i,
                            address,
                        }
                    };

                    let block_index = blocks.len() as u32;
                    index_to_block.insert(block_start, block_index);

                    blocks.push(CodeBlock {
                        instruction_slice: block_start..i + 1,
                        terminator: Some(term),
                        // Store instruction's index temporary
                        fallthrough_to: Some(i + 1),
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
                *to = index_to_block[to];
            }
        }

        Self { blocks }
    }
}
