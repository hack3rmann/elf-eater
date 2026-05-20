use crate::{
    algo,
    asm::passes::{
        code_flow::{BlockId, FunctionCodeFlow, InstructionSpan},
        references::FunctionInfo,
        semantic::{
            ArithmeticInstruction, ArithmeticOperands, ExtendedGpRegister, GpRegister,
            RegOrConst64, RegOrMemory, Register64, RegisterSliceKind, Registers64,
            SemanticInstruction, SizedRegOrMemory,
        },
    },
};
use petgraph::{
    algo::dominators::{self, Dominators},
    graph::{DiGraph, NodeIndex},
};
use smallvec::{SmallVec, smallvec};
use std::array;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefinitionTarget {
    Register(Register64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

impl ValueId {
    pub const INVALID: Self = Self(u32::MAX);
}

impl Default for ValueId {
    fn default() -> Self {
        Self::INVALID
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueType {
    Register(Register64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    External,
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Definition {
    pub id: ValueId,
    pub target: DefinitionTarget,
    pub value: DefinitionValue,
    pub span: InstructionSpan,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefinitionId(pub u32);

impl DefinitionId {
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaBlock {
    pub definitions: SmallVec<[DefinitionId; 14]>,
}

#[derive(Clone, Debug)]
pub struct Ssa {
    pub cfg: DiGraph<BlockId, bool>,
    pub dom_tree: Dominators<NodeIndex>,
    pub dom_frontier: Vec<SmallVec<[NodeIndex; 4]>>,
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
            array::from_fn(|_| SmallVec::<[BlockId; 12]>::new_const());

        for (block_id, block) in flow.blocks() {
            let instructions = &info.instructions[block.span.range()];

            for &instruction in instructions {
                visit_assignment(instruction, &mut |destination, _| {
                    def_sources[destination as usize].push(block_id);
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

        let mut values = Vec::<Value>::new();
        let mut blocks = vec![SsaBlock::default(); flow.blocks.len()];
        let mut definitions = Vec::<Definition>::new();

        let mut next_value = make_counter(ValueId);
        let mut next_def = make_counter(DefinitionId);

        for (block_index, &regs) in phi_needed.iter().enumerate() {
            for reg in regs {
                let Some(reg) = reg.single() else {
                    unreachable!();
                };

                let value = next_value();
                values.push(Value {
                    id: value,
                    ty: ValueType::Register(reg),
                });

                let def_id = next_def();
                definitions.push(Definition {
                    id: value,
                    value: DefinitionValue::Phi {
                        dependencies: smallvec![],
                    },
                    span: InstructionSpan::EMPTY,
                    target: DefinitionTarget::Register(reg),
                });

                blocks[block_index].definitions.push(def_id);
            }
        }

        let mut idom_inverse: Vec<SmallVec<[u32; 12]>> = vec![smallvec![]; flow.blocks.len()];

        for index in (0..flow.blocks.len()).map(NodeIndex::new) {
            let Some(dom_id) = dom_tree.immediate_dominator(index) else {
                continue;
            };

            idom_inverse[dom_id.index()].push(index.index() as u32);
        }

        type DefStacks = Vec<[ValueId; Register64::COUNT]>;

        let mut def_stack: DefStacks = vec![[ValueId::INVALID; Register64::COUNT]];

        for reg in Register64::ALL {
            let root_value_id = next_value();
            values.push(Value {
                id: root_value_id,
                ty: ValueType::Register(reg),
            });

            def_stack[0][reg as usize] = root_value_id;

            let def_id = next_def();
            definitions.push(Definition {
                id: root_value_id,
                target: DefinitionTarget::Register(reg),
                value: DefinitionValue::External,
                span: InstructionSpan::EMPTY,
            });

            if let Some(first) = blocks.first_mut() {
                first.definitions.push(def_id);
            }
        }

        let mut finalized_defs: Vec<[ValueId; Register64::COUNT]> =
            vec![[ValueId::INVALID; Register64::COUNT]; flow.blocks.len()];

        let mut enter = |def_stack: &mut DefStacks, node_id| {
            let parent_def = def_stack.last().copied().expect("def_stack is never empty");
            let mut last_def = parent_def;

            let phis = blocks[node_id as usize]
                .definitions
                .iter()
                .map(|&id| &definitions[id.index()])
                .filter(|def| matches!(def.value, DefinitionValue::Phi { .. }));

            // The node contains Phi for the selected register
            for def in phis {
                let DefinitionTarget::Register(reg) = def.target;
                last_def[reg as usize] = def.id;
            }

            let block = &flow.blocks[node_id as usize];
            let block_instructions = &info.instructions[block.span.range()];
            let block_start = block.span.range().start as u32;

            for (&instruction, i) in block_instructions.iter().zip(block_start..) {
                let span = InstructionSpan::new(i, i + 1);

                visit_assignment(instruction, &mut |target, def_value| {
                    let value = next_value();
                    values.push(Value {
                        id: value,
                        ty: ValueType::Register(target),
                    });

                    last_def[target as usize] = value;

                    // FIXME: Missing the actual RHS
                    let def_id = next_def();
                    definitions.push(Definition {
                        id: value,
                        value: def_value,
                        span,
                        target: DefinitionTarget::Register(target),
                    });

                    blocks[node_id as usize].definitions.push(def_id);
                });
            }

            def_stack.push(last_def);
            finalized_defs[node_id as usize] = last_def;
        };

        let mut leave = |def_stack: &mut DefStacks, _| {
            def_stack.pop();
        };

        iterative_dfs(&idom_inverse, 0, &mut def_stack, &mut enter, &mut leave);

        iterative_dfs(
            &idom_inverse,
            0,
            &mut (),
            &mut |_, node_id| {
                for &def_id in &blocks[node_id as usize].definitions {
                    let defs = &mut definitions[def_id.index()];

                    let DefinitionTarget::Register(phi_reg) = defs.target;
                    let DefinitionValue::Phi { dependencies } = &mut defs.value else {
                        break;
                    };

                    for parent in cfg.neighbors_directed(
                        NodeIndex::new(node_id as usize),
                        petgraph::Direction::Incoming,
                    ) {
                        let parent_value_id = finalized_defs[parent.index()];

                        dependencies.push(Dependency {
                            value: parent_value_id[phi_reg as usize],
                            source: BlockId(parent.index() as u32),
                        });
                    }
                }
            },
            &mut |_, _| {},
        );

        Self {
            values,
            blocks,
            definitions,
            cfg,
            dom_tree,
            dom_frontier,
        }
    }
}

fn iterative_dfs<S>(
    graph: &[SmallVec<[u32; 12]>],
    root: u32,
    state: &mut S,
    enter: &mut impl FnMut(&mut S, u32),
    leave: &mut impl FnMut(&mut S, u32),
) {
    let mut stack = vec![(root, 0)];

    while let Some((node, i)) = stack.pop() {
        if i == 0 {
            enter(state, node);
        }

        if i >= graph[node as usize].len() {
            leave(state, node);
            continue;
        }

        let child = graph[node as usize][i];

        stack.push((node, i + 1));
        stack.push((child, 0));
    }
}

#[inline(always)]
fn make_counter<T>(id: impl Fn(u32) -> T) -> impl FnMut() -> T {
    let mut last_id = u32::MAX;
    move || {
        last_id = last_id.wrapping_add(1);
        id(last_id)
    }
}

fn visit_assignment(
    instruction: SemanticInstruction,
    visit: &mut impl FnMut(Register64, DefinitionValue),
) {
    match instruction {
        SemanticInstruction::Assignment {
            destination,
            source: RegOrConst64::Const(source),
            slice: RegisterSliceKind::R64,
        } => {
            visit(destination, DefinitionValue::Const(source));
        }
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
            visit(destination, DefinitionValue::Const(42));
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
            visit(first, DefinitionValue::Const(42));
            visit(second, DefinitionValue::Const(42));
        }
        _ => (),
    }
}
