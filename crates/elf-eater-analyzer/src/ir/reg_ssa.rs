use crate::{
    algo,
    asm::passes::{
        code_flow::{BlockId, FunctionCodeFlow, InstructionSpan},
        references::FunctionInfo,
        semantic::{
            ArithmeticInstruction, ArithmeticOperands, ExtendedGpRegister, GpRegister, RegOrMemory,
            Register64, Registers64, SemanticInstruction, SizedRegOrMemory,
        },
    },
};
use petgraph::{algo::dominators, graph::NodeIndex};
use smallvec::SmallVec;
use std::array;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueType {
    #[default]
    Register,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Value {
    pub id: ValueId,
    pub ty: ValueType,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dependency {
    pub value: ValueId,
    pub source: BlockId,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefinitionId(pub u32);

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaBlock {
    pub definitions: SmallVec<[DefinitionId; 14]>,
}

pub struct Ssa {
    pub values: Vec<Value>,
    pub definitions: Vec<Definition>,
    pub blocks: Vec<SsaBlock>,
}

impl Ssa {
    pub fn build(flow: &FunctionCodeFlow, info: &FunctionInfo) -> Self {
        let cfg = flow.build_cfg();

        let dom_tree = dominators::simple_fast(&cfg, NodeIndex::new(0));
        let dom_frontier = algo::dominance_frontiers(&cfg, &dom_tree);

        let mut def_sources: [_; Register64::COUNT] =
            array::from_fn(|_| SmallVec::<[BlockId; 6]>::new_const());

        let mut block_assignments = vec![Registers64::empty(); flow.blocks.len()];

        for (block_id, block) in flow.blocks() {
            let instructions = &info.instructions[block.span.range()];

            for &instruction in instructions {
                visit_assignment(instruction, &mut |destination| {
                    def_sources[destination as usize].push(block_id);
                    block_assignments[block_id.index()].insert(destination.into());
                });
            }
        }

        let mut phi_needed = vec![Registers64::empty(); flow.blocks.len()];
        let mut extended_def = Vec::new();

        for reg in Register64::ALL {
            extended_def.clear();
            extended_def.extend_from_slice(&def_sources[reg as usize]);

            while let Some(def_block) = extended_def.pop() {
                for join in &dom_frontier[def_block.0 as usize] {
                    let block_id = BlockId(join.index() as u32);
                    let bits = &mut phi_needed[block_id.index()];

                    if !bits.contains(reg.into()) {
                        bits.insert(reg.into());
                        extended_def.push(block_id);
                    }
                }
            }
        }

        dbg!(&phi_needed);
        dbg!(&block_assignments);

        todo!()
    }
}

fn visit_assignment(instruction: SemanticInstruction, visit: &mut impl FnMut(Register64)) {
    match instruction {
        SemanticInstruction::LoadAddress {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        }
        | SemanticInstruction::Pop {
            operand: RegOrMemory::Reg(destination),
            ..
        }
        | SemanticInstruction::LoadZeroExtend {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        }
        | SemanticInstruction::LoadSignExtend {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        }
        | SemanticInstruction::Exchange {
            first: RegOrMemory::Reg(destination),
            second: RegOrMemory::Mem(_),
            ..
        }
        | SemanticInstruction::Exchange {
            first: RegOrMemory::Mem(_),
            second: RegOrMemory::Reg(destination),
            ..
        }
        | SemanticInstruction::Load {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        }
        | SemanticInstruction::Assignment { destination, .. }
        | SemanticInstruction::AssignmentSignExtend {
            destination:
                ExtendedGpRegister {
                    hi: None,
                    lo:
                        GpRegister {
                            full: destination, ..
                        },
                },
            ..
        }
        | SemanticInstruction::Arithmetic(ArithmeticInstruction {
            operands:
                ArithmeticOperands::ShortExpression {
                    result:
                        SizedRegOrMemory::Reg(GpRegister {
                            full: destination, ..
                        }),
                    ..
                }
                | ArithmeticOperands::TernaryExpression {
                    result:
                        SizedRegOrMemory::Reg(GpRegister {
                            full: destination, ..
                        }),
                    ..
                },
            ..
        })
        | SemanticInstruction::AssignmentZeroExtend {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        } => {
            visit(destination);
        }
        SemanticInstruction::Exchange {
            first: RegOrMemory::Reg(first),
            second: RegOrMemory::Reg(second),
            ..
        }
        | SemanticInstruction::Arithmetic(ArithmeticInstruction {
            operands:
                ArithmeticOperands::ResultExtendedExpression {
                    result_hi: SizedRegOrMemory::Reg(GpRegister { full: first, .. }),
                    result_lo: SizedRegOrMemory::Reg(GpRegister { full: second, .. }),
                    ..
                }
                | ArithmeticOperands::OperandExtendedExpression {
                    result_first: SizedRegOrMemory::Reg(GpRegister { full: first, .. }),
                    result_second: SizedRegOrMemory::Reg(GpRegister { full: second, .. }),
                    ..
                },
            ..
        })
        | SemanticInstruction::AssignmentSignExtend {
            destination:
                ExtendedGpRegister {
                    hi: Some(GpRegister { full: first, .. }),
                    lo: GpRegister { full: second, .. },
                },
            ..
        } => {
            visit(first);
            visit(second);
        }
        _ => (),
    }
}
