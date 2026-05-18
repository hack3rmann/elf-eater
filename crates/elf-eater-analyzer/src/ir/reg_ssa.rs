use crate::{
    algo,
    asm::passes::{
        code_flow::{BlockId, FunctionCodeFlow, InstructionSpan},
        references::FunctionInfo,
        semantic::{Register64, SemanticInstruction},
    },
};
use petgraph::{algo::dominators, graph::NodeIndex};
use smallvec::SmallVec;
use std::{
    array,
    collections::{BTreeMap, BTreeSet, HashSet},
};

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

        let mut block_assignments = BTreeMap::<BlockId, HashSet<Register64>>::new();

        for (block_id, block) in flow.blocks() {
            let instructions = &info.instructions[block.span.range()];

            for instruction in instructions {
                if let &SemanticInstruction::Assignment { destination, .. } = instruction {
                    def_sources[destination as usize].push(block_id);

                    block_assignments
                        .entry(block_id)
                        .or_default()
                        .insert(destination);
                }
            }
        }

        let mut phi_needed = BTreeSet::new();
        let mut extended_def = Vec::new();

        for reg in Register64::ALL {
            extended_def.clear();
            extended_def.extend_from_slice(&def_sources[reg as usize]);

            while let Some(def_block) = extended_def.pop() {
                for join in &dom_frontier[def_block.0 as usize] {
                    let block_id = BlockId(join.index() as u32);

                    if phi_needed.insert((block_id, reg)) {
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
