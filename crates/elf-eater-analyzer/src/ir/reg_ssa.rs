use crate::{
    algo,
    asm::passes::{
        code_flow::{BlockId, FunctionCodeFlow, InstructionSpan},
        references::FunctionInfo,
        semantic::{
            ArithmeticInstruction, ArithmeticOperands, ExtendedGpRegister, GpRegister,
            MemoryExpression, Operand, PointerSize, RegOrConst, RegOrConst64, RegOrMemory,
            Register64, RegisterSliceKind, Registers64, SemanticInstruction, SizedRegOrMemory,
        },
    },
};
use petgraph::{
    algo::dominators::{self, Dominators},
    graph::{DiGraph, NodeIndex},
};
use smallvec::{SmallVec, smallvec};
use std::{
    array,
    fmt::{self, Display},
};

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

impl Display for ValueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::INVALID {
            return f.write_str("nil");
        }

        write!(f, "x{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueSize {
    #[default]
    U64 = 64,
    U32 = 32,
    U16 = 16,
    U8 = 8,
}

impl ValueSize {
    pub const fn as_str(self) -> &'static str {
        match self {
            ValueSize::U64 => "64",
            ValueSize::U32 => "32",
            ValueSize::U16 => "16",
            ValueSize::U8 => "8",
        }
    }
}

impl TryFrom<PointerSize> for ValueSize {
    type Error = ();

    fn try_from(value: PointerSize) -> Result<Self, Self::Error> {
        Ok(match value {
            PointerSize::Byte => Self::U8,
            PointerSize::Word => Self::U16,
            PointerSize::Dword => Self::U32,
            PointerSize::Qword => Self::U64,
            _ => return Err(()),
        })
    }
}

impl From<RegisterSliceKind> for ValueSize {
    fn from(value: RegisterSliceKind) -> Self {
        match value {
            RegisterSliceKind::R64 => Self::U64,
            RegisterSliceKind::R32 => Self::U32,
            RegisterSliceKind::R16 => Self::U16,
            RegisterSliceKind::H8 | RegisterSliceKind::L8 => Self::U8,
        }
    }
}

impl Display for ValueSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueType {
    Temporary { size: ValueSize },
    Register(Register64),
    Memory,
}

impl Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temporary { size } => write!(f, "tmp{}", *size as u8),
            Self::Register(reg) => reg.fmt(f),
            Self::Memory => f.write_str("mem"),
        }
    }
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

impl Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x{}@b{}", self.value.0, self.source.index())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmRef {
    Asm(RegOrConst64),
    Memory,
    Ssa(ValueId),
}

impl From<ValueSource> for AsmRef {
    fn from(value: ValueSource) -> Self {
        match value {
            ValueSource::Const(c) => AsmRef::Asm(RegOrConst64::Const(c)),
            ValueSource::Value(id) => AsmRef::Ssa(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmVarType {
    Register(Register64),
    Memory,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BitPosition {
    /// reg[32..64]
    #[default]
    From32To64,
    /// reg[48..64]
    From48To64,
    /// reg[48..56]
    From48To56,
    /// reg[56..64]
    From56To64,
}

impl BitPosition {
    pub const fn as_str(self) -> &'static str {
        match self {
            BitPosition::From32To64 => "32..64",
            BitPosition::From48To64 => "48..64",
            BitPosition::From48To56 => "48..56",
            BitPosition::From56To64 => "56..64",
        }
    }

    pub const fn size(self) -> ValueSize {
        match self {
            BitPosition::From32To64 => ValueSize::U32,
            BitPosition::From48To64 => ValueSize::U16,
            BitPosition::From48To56 | BitPosition::From56To64 => ValueSize::U8,
        }
    }
}

impl TryFrom<RegisterSliceKind> for BitPosition {
    type Error = ();

    fn try_from(value: RegisterSliceKind) -> Result<Self, Self::Error> {
        Ok(match value {
            RegisterSliceKind::R32 => Self::From32To64,
            RegisterSliceKind::R16 => Self::From48To64,
            RegisterSliceKind::H8 => Self::From48To56,
            RegisterSliceKind::L8 => Self::From56To64,
            RegisterSliceKind::R64 => return Err(()),
        })
    }
}

impl From<BitPosition> for RegisterSliceKind {
    fn from(value: BitPosition) -> Self {
        match value {
            BitPosition::From32To64 => Self::R32,
            BitPosition::From48To64 => Self::R16,
            BitPosition::From48To56 => Self::H8,
            BitPosition::From56To64 => Self::L8,
        }
    }
}

impl TryFrom<ValueSize> for BitPosition {
    type Error = ();

    fn try_from(value: ValueSize) -> Result<Self, Self::Error> {
        Ok(match value {
            ValueSize::U32 => Self::From32To64,
            ValueSize::U16 => Self::From48To64,
            ValueSize::U8 => Self::From56To64,
            ValueSize::U64 => return Err(()),
        })
    }
}

impl Display for BitPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmDefinitionValue {
    Value(AsmRef),
    Add {
        left: AsmRef,
        right: AsmRef,
    },
    Sub {
        left: AsmRef,
        right: AsmRef,
    },
    Mul {
        left: AsmRef,
        right: AsmRef,
    },
    Load {
        size: ValueSize,
        address: AsmRef,
    },
    Store {
        size: ValueSize,
        address: AsmRef,
        value: AsmRef,
    },
    BitInsert {
        destination: AsmRef,
        source: AsmRef,
        position: BitPosition,
    },
    BitCut {
        value: AsmRef,
        position: BitPosition,
    },
}

impl AsmDefinitionValue {
    pub fn resolve(self, resolve_var: impl Fn(AsmVarType) -> ValueId) -> DefinitionValue {
        let resolve = |value: AsmRef| -> ValueSource {
            match value {
                AsmRef::Asm(RegOrConst64::Reg(reg)) => {
                    ValueSource::Value(resolve_var(AsmVarType::Register(reg)))
                }
                AsmRef::Memory => ValueSource::Value(resolve_var(AsmVarType::Memory)),
                AsmRef::Asm(RegOrConst64::Const(c)) => ValueSource::Const(c),
                AsmRef::Ssa(id) => ValueSource::Value(id),
            }
        };

        match self {
            Self::Value(value) => DefinitionValue::Value(resolve(value)),
            Self::Add { left, right } => DefinitionValue::Add {
                left: resolve(left),
                right: resolve(right),
            },
            Self::Sub { left, right } => DefinitionValue::Sub {
                left: resolve(left),
                right: resolve(right),
            },
            Self::Mul { left, right } => DefinitionValue::Mul {
                left: resolve(left),
                right: resolve(right),
            },
            Self::Load { address, size } => DefinitionValue::Load {
                memory: resolve_var(AsmVarType::Memory),
                address: resolve(address),
                size,
            },
            Self::Store {
                address,
                value,
                size,
            } => DefinitionValue::Store {
                memory: resolve_var(AsmVarType::Memory),
                address: resolve(address),
                value: resolve(value),
                size,
            },
            Self::BitInsert {
                destination,
                source,
                position,
            } => DefinitionValue::BitInsert {
                destination: resolve(destination),
                source: resolve(source),
                position,
            },
            Self::BitCut { value, position } => DefinitionValue::BitCut {
                value: resolve(value),
                position,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueSource {
    Const(u64),
    Value(ValueId),
}

impl Display for ValueSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(c) => c.fmt(f),
            Self::Value(id) => id.fmt(f),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefinitionValue {
    #[default]
    Undefined,
    External,
    Value(ValueSource),
    Add {
        left: ValueSource,
        right: ValueSource,
    },
    Sub {
        left: ValueSource,
        right: ValueSource,
    },
    Mul {
        left: ValueSource,
        right: ValueSource,
    },
    Load {
        size: ValueSize,
        memory: ValueId,
        address: ValueSource,
    },
    Store {
        size: ValueSize,
        memory: ValueId,
        address: ValueSource,
        value: ValueSource,
    },
    BitInsert {
        destination: ValueSource,
        source: ValueSource,
        position: BitPosition,
    },
    BitCut {
        value: ValueSource,
        position: BitPosition,
    },
    Phi {
        dependencies: SmallVec<[Dependency; 3]>,
    },
}

impl Display for DefinitionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefinitionValue::Undefined => f.write_str("undefined"),
            DefinitionValue::External => f.write_str("external"),
            DefinitionValue::Value(source) => source.fmt(f),
            DefinitionValue::Add { left, right } => write!(f, "add({left}, {right})"),
            DefinitionValue::Sub { left, right } => write!(f, "sub({left}, {right})"),
            DefinitionValue::Mul { left, right } => write!(f, "mul({left}, {right})"),
            DefinitionValue::Load {
                memory,
                address,
                size,
            } => write!(f, "load{size}({memory}, {address})"),
            DefinitionValue::Store {
                size,
                memory,
                address,
                value,
            } => write!(f, "store{size}({memory}, {address}, {value})"),
            DefinitionValue::BitInsert {
                destination,
                source,
                position,
            } => write!(f, "insert({destination}, {source}, {position})"),
            DefinitionValue::BitCut { value, position } => write!(f, "cut({value}, {position})"),
            DefinitionValue::Phi { dependencies } => {
                f.write_str("phi(")?;

                if !dependencies.is_empty() {
                    for dep in &dependencies[..1] {
                        dep.fmt(f)?;
                    }

                    for dep in &dependencies[1..] {
                        write!(f, ", {dep}")?;
                    }
                }

                f.write_str(")")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Definition {
    pub id: ValueId,
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

        const VARIABLE_COUNT: usize = Register64::COUNT + 1;
        const VAR_MEMORY: usize = Register64::COUNT;

        let mut def_sources: [_; VARIABLE_COUNT] =
            array::from_fn(|_| SmallVec::<[BlockId; 12]>::new_const());

        for (block_id, block) in flow.blocks() {
            let instructions = &info.instructions[block.span.range()];

            for &instruction in instructions {
                visit_assignment(instruction, &mut |destination, _| {
                    match destination {
                        AsmVisitTarget::NewValue { size: _ } => {}
                        AsmVisitTarget::Memory => {
                            def_sources[VAR_MEMORY].push(block_id);
                        }
                        AsmVisitTarget::Register(reg) => {
                            def_sources[reg as usize].push(block_id);
                        }
                    }

                    ValueId::INVALID
                });
            }
        }

        #[derive(Clone, Copy)]
        struct PhiTarget {
            pub registers: Registers64,
            pub has_memory: bool,
        }

        const PHI_TARGET_EMPTY: PhiTarget = PhiTarget {
            registers: Registers64::empty(),
            has_memory: false,
        };

        let mut phi_needed = vec![PHI_TARGET_EMPTY; flow.blocks.len()];
        let mut extended_def = Vec::new();

        for reg in Register64::ALL {
            extended_def.clear();
            extended_def.extend_from_slice(&def_sources[reg as usize]);

            while let Some(def_block) = extended_def.pop() {
                for &join in &dom_frontier[def_block.index()] {
                    let block_id = BlockId(join.index() as u32);
                    let bits = &mut phi_needed[block_id.index()].registers;

                    if !bits.contains(reg.into()) {
                        bits.insert(reg.into());
                        extended_def.push(block_id);
                    }
                }
            }
        }

        // Memory pass
        extended_def.clear();
        extended_def.extend_from_slice(&def_sources[VAR_MEMORY]);

        while let Some(def_block) = extended_def.pop() {
            for &join in &dom_frontier[def_block.index()] {
                let block_id = BlockId(join.index() as u32);
                let has_memory = &mut phi_needed[block_id.index()].has_memory;

                if !*has_memory {
                    *has_memory = true;
                    extended_def.push(block_id);
                }
            }
        }

        let mut values = Vec::<Value>::new();
        let mut blocks = vec![SsaBlock::default(); flow.blocks.len()];
        let mut definitions = Vec::<Definition>::new();

        let mut next_value = make_counter(ValueId);
        let mut next_def = make_counter(DefinitionId);

        for (block_index, &targets) in phi_needed.iter().enumerate() {
            let targets = targets
                .registers
                .iter()
                .flat_map(Registers64::single)
                .map(ValueType::Register)
                .chain(targets.has_memory.then_some(ValueType::Memory));

            for ty in targets {
                let id = next_value();
                values.push(Value { id, ty });

                let def_id = next_def();
                definitions.push(Definition {
                    id,
                    value: DefinitionValue::Phi {
                        dependencies: smallvec![],
                    },
                    span: InstructionSpan::EMPTY,
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

        type DefStacks = Vec<[ValueId; VARIABLE_COUNT]>;

        let mut def_stack: DefStacks = vec![[ValueId::INVALID; VARIABLE_COUNT]];

        let variables = Register64::ALL
            .into_iter()
            .map(|reg| (reg as usize, reg))
            .map(|(i, reg)| (i, ValueType::Register(reg)))
            .chain([(VAR_MEMORY, ValueType::Memory)]);

        for (var_index, ty) in variables {
            let root_value_id = next_value();
            values.push(Value {
                id: root_value_id,
                ty,
            });

            def_stack[0][var_index] = root_value_id;

            let def_id = next_def();
            definitions.push(Definition {
                id: root_value_id,
                value: DefinitionValue::External,
                span: InstructionSpan::EMPTY,
            });

            if let Some(first) = blocks.first_mut() {
                first.definitions.push(def_id);
            }
        }

        let mut finalized_defs: Vec<[ValueId; VARIABLE_COUNT]> =
            vec![[ValueId::INVALID; VARIABLE_COUNT]; flow.blocks.len()];

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
                let value = values[def.id.0 as usize];

                let var_index = match value.ty {
                    ValueType::Temporary { size: _ } => continue,
                    ValueType::Register(reg) => reg as usize,
                    ValueType::Memory => VAR_MEMORY,
                };

                last_def[var_index] = def.id;
            }

            let block = &flow.blocks[node_id as usize];
            let block_instructions = &info.instructions[block.span.range()];
            let block_start = block.span.range().start as u32;

            for (&instruction, i) in block_instructions.iter().zip(block_start..) {
                let span = InstructionSpan::new(i, i + 1);

                visit_assignment(instruction, &mut |target, def_value| {
                    let value_type = match target {
                        AsmVisitTarget::NewValue { size } => ValueType::Temporary { size },
                        AsmVisitTarget::Memory => ValueType::Memory,
                        AsmVisitTarget::Register(reg) => ValueType::Register(reg),
                    };

                    let value_id = next_value();
                    values.push(Value {
                        id: value_id,
                        ty: value_type,
                    });

                    let def_id = next_def();
                    definitions.push(Definition {
                        id: value_id,
                        value: def_value.resolve(|var| match var {
                            AsmVarType::Register(reg) => last_def[reg as usize],
                            AsmVarType::Memory => last_def[VAR_MEMORY],
                        }),
                        span,
                    });

                    // Update the last definition only after name resolution
                    match target {
                        AsmVisitTarget::Memory => {
                            last_def[VAR_MEMORY] = value_id;
                        }
                        AsmVisitTarget::Register(reg) => {
                            last_def[reg as usize] = value_id;
                        }
                        AsmVisitTarget::NewValue { size: _ } => {}
                    }

                    blocks[node_id as usize].definitions.push(def_id);

                    value_id
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
                    let value = values[defs.id.0 as usize];

                    // Break, because first 0..N definitions are always phi entries for variables
                    let var_index = match value.ty {
                        ValueType::Register(reg) => reg as usize,
                        ValueType::Memory => VAR_MEMORY,
                        ValueType::Temporary { size: _ } => break,
                    };
                    let DefinitionValue::Phi { dependencies } = &mut defs.value else {
                        break;
                    };

                    for parent in cfg.neighbors_directed(
                        NodeIndex::new(node_id as usize),
                        petgraph::Direction::Incoming,
                    ) {
                        let parent_value_id = finalized_defs[parent.index()];

                        dependencies.push(Dependency {
                            value: parent_value_id[var_index],
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmVisitTarget {
    NewValue { size: ValueSize },
    Memory,
    Register(Register64),
}

fn visit_memory_expr(
    expr: MemoryExpression,
    target: AsmVisitTarget,
    visit: &mut impl FnMut(AsmVisitTarget, AsmDefinitionValue) -> ValueId,
) -> ValueSource {
    match expr {
        MemoryExpression::Absolute {
            base: Some(base),
            index: Some(index),
            scale,
            displacement,
        } => {
            let index_times_scale = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                AsmDefinitionValue::Mul {
                    left: AsmRef::Asm(RegOrConst64::Reg(index)),
                    right: AsmRef::Asm(RegOrConst64::Const(scale as u64)),
                },
            );

            let base_plus_scaled_index = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                AsmDefinitionValue::Add {
                    left: AsmRef::Asm(RegOrConst64::Reg(base)),
                    right: AsmRef::Ssa(index_times_scale),
                },
            );

            let left = AsmRef::Ssa(base_plus_scaled_index);

            let value = if displacement > 0 {
                AsmDefinitionValue::Add {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(displacement as u64)),
                }
            } else {
                AsmDefinitionValue::Sub {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(-displacement as u64)),
                }
            };

            let id = visit(target, value);

            ValueSource::Value(id)
        }
        MemoryExpression::Absolute {
            base: None,
            index: Some(index),
            scale,
            displacement,
        } => {
            let index_times_scale = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                // TODO(hack3rmann): don't emit mul(x, 1)
                AsmDefinitionValue::Mul {
                    left: AsmRef::Asm(RegOrConst64::Reg(index)),
                    right: AsmRef::Asm(RegOrConst64::Const(scale as u64)),
                },
            );

            let left = AsmRef::Ssa(index_times_scale);

            let value = if displacement > 0 {
                AsmDefinitionValue::Add {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(displacement as u64)),
                }
            } else {
                AsmDefinitionValue::Sub {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(-displacement as u64)),
                }
            };

            let id = visit(target, value);

            ValueSource::Value(id)
        }
        MemoryExpression::Absolute {
            base: Some(base),
            index: None,
            scale: _,
            displacement,
        } => {
            let left = AsmRef::Asm(RegOrConst64::Reg(base));

            let value = if displacement > 0 {
                AsmDefinitionValue::Add {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(displacement as u64)),
                }
            } else {
                AsmDefinitionValue::Sub {
                    left,
                    right: AsmRef::Asm(RegOrConst64::Const(-displacement as u64)),
                }
            };

            let id = visit(target, value);

            ValueSource::Value(id)
        }
        MemoryExpression::Absolute {
            base: None,
            index: None,
            scale: _,
            displacement,
        } => {
            if let AsmVisitTarget::Register(_) = target {
                visit(
                    target,
                    AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Const(
                        displacement as u64,
                    ))),
                );
            }

            ValueSource::Const(displacement as u64)
        }
        // FIXME(hack3rmann): actually depends on `RIP`
        MemoryExpression::Relative { address } => {
            if let AsmVisitTarget::Register(_) = target {
                visit(
                    target,
                    AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Const(address))),
                );
            }

            ValueSource::Const(address)
        }
    }
}

fn visit_assignment(
    instruction: SemanticInstruction,
    visit: &mut impl FnMut(AsmVisitTarget, AsmDefinitionValue) -> ValueId,
) {
    match instruction {
        SemanticInstruction::Assignment {
            destination,
            source,
            slice: RegisterSliceKind::R64,
        } => {
            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::Value(AsmRef::Asm(source)),
            );
        }
        SemanticInstruction::Assignment {
            destination,
            source,
            slice,
        } => {
            let position = BitPosition::try_from(slice).expect("R64 variant is handled");

            let source = match source {
                RegOrConst64::Reg(reg) => AsmRef::Ssa(visit(
                    AsmVisitTarget::NewValue {
                        size: position.size(),
                    },
                    AsmDefinitionValue::BitCut {
                        value: AsmRef::Asm(RegOrConst64::Reg(reg)),
                        position,
                    },
                )),
                RegOrConst64::Const(c) => AsmRef::Asm(RegOrConst64::Const(c)),
            };

            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(destination)),
                    source,
                    position,
                },
            );
        }
        SemanticInstruction::Exchange {
            first: RegOrMemory::Reg(first),
            second: RegOrMemory::Reg(second),
            slice: RegisterSliceKind::R64,
        } => {
            // xchg A, B
            //  =>
            // tmp = A
            // A = B
            // B = tmp

            let tmp = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Reg(first))),
            );

            visit(
                AsmVisitTarget::Register(first),
                AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Reg(second))),
            );

            visit(
                AsmVisitTarget::Register(second),
                AsmDefinitionValue::Value(AsmRef::Ssa(tmp)),
            );
        }
        SemanticInstruction::Exchange {
            first: RegOrMemory::Reg(first),
            second: RegOrMemory::Reg(second),
            slice,
        } => {
            // xchg A, B
            //  =>
            // a = cut(A, 32..64)
            // b = cut(B, 32..64)
            // A = insert(A, b, 32..64)
            // B = insert(B, a, 32..64)

            let position = BitPosition::try_from(slice).expect("R64 is handled");

            let first_cut = visit(
                AsmVisitTarget::NewValue {
                    size: position.size(),
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Asm(RegOrConst64::Reg(first)),
                    position,
                },
            );

            let second_cut = visit(
                AsmVisitTarget::NewValue {
                    size: position.size(),
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Asm(RegOrConst64::Reg(second)),
                    position,
                },
            );

            visit(
                AsmVisitTarget::Register(first),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(first)),
                    source: AsmRef::Ssa(second_cut),
                    position,
                },
            );

            visit(
                AsmVisitTarget::Register(second),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(second)),
                    source: AsmRef::Ssa(first_cut),
                    position,
                },
            );
        }
        SemanticInstruction::Exchange {
            first: RegOrMemory::Mem(mem),
            second: RegOrMemory::Reg(reg),
            slice,
        }
        | SemanticInstruction::Exchange {
            first: RegOrMemory::Reg(reg),
            second: RegOrMemory::Mem(mem),
            slice,
        } => {
            // xchg reg16, qword [mem]
            //  =>
            // address = { mem }
            // value = load16(address)
            // reg_slice = cut(regN, 48..64)
            // store16(address, reg_slice)
            // reg16 = insert(reg64, value, 48..64)

            let address = visit_memory_expr(
                mem,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            let size = ValueSize::from(slice);

            let value = visit(
                AsmVisitTarget::NewValue { size },
                AsmDefinitionValue::Load {
                    size,
                    address: address.into(),
                },
            );

            let reg_slice = match BitPosition::try_from(slice) {
                Ok(position) => {
                    let reg_slice = visit(
                        AsmVisitTarget::NewValue { size },
                        AsmDefinitionValue::BitCut {
                            value: AsmRef::Asm(RegOrConst64::Reg(reg)),
                            position,
                        },
                    );
                    AsmRef::Ssa(reg_slice)
                }
                Err(()) => AsmRef::Asm(RegOrConst64::Reg(reg)),
            };

            visit(
                AsmVisitTarget::Memory,
                AsmDefinitionValue::Store {
                    size,
                    address: address.into(),
                    value: reg_slice,
                },
            );

            match BitPosition::try_from(slice) {
                Err(()) => visit(
                    AsmVisitTarget::Register(reg),
                    AsmDefinitionValue::Value(AsmRef::Ssa(value)),
                ),
                // NOTE(hack3rmann): zero-extends to reg64
                Ok(position @ BitPosition::From32To64) => visit(
                    AsmVisitTarget::Register(reg),
                    AsmDefinitionValue::BitInsert {
                        destination: AsmRef::Asm(RegOrConst64::Const(0)),
                        source: AsmRef::Ssa(value),
                        position,
                    },
                ),
                Ok(position) => visit(
                    AsmVisitTarget::Register(reg),
                    AsmDefinitionValue::BitInsert {
                        destination: AsmRef::Asm(RegOrConst64::Reg(reg)),
                        source: AsmRef::Ssa(value),
                        position,
                    },
                ),
            };
        }
        SemanticInstruction::Exchange {
            first: RegOrMemory::Mem(_),
            second: RegOrMemory::Mem(_),
            slice: _,
        } => {
            unimplemented!("unimplemented `xchg mem, mem`")
        }
        SemanticInstruction::LoadAddress {
            destination:
                GpRegister {
                    full: destination,
                    slice: RegisterSliceKind::R64,
                },
            expr,
        } => {
            visit_memory_expr(expr, AsmVisitTarget::Register(destination), visit);
        }
        SemanticInstruction::LoadAddress { destination, expr } => {
            let position =
                BitPosition::try_from(destination.slice).expect("R64 variant is handled");

            let address = visit_memory_expr(
                expr,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            visit(
                AsmVisitTarget::Register(destination.full),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(destination.full)),
                    source: address.into(),
                    position,
                },
            );
        }
        SemanticInstruction::Load {
            destination,
            source,
        } => {
            let address = visit_memory_expr(
                source,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            let (value, position) = match BitPosition::try_from(destination.slice) {
                Ok(position) => (
                    visit(
                        AsmVisitTarget::NewValue {
                            size: position.size(),
                        },
                        AsmDefinitionValue::Load {
                            address: AsmRef::from(address),
                            size: position.size(),
                        },
                    ),
                    position,
                ),
                Err(()) => {
                    visit(
                        AsmVisitTarget::Register(destination.full),
                        AsmDefinitionValue::Load {
                            address: AsmRef::from(address),
                            size: ValueSize::U64,
                        },
                    );
                    return;
                }
            };

            visit(
                AsmVisitTarget::Register(destination.full),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(destination.full)),
                    source: AsmRef::Ssa(value),
                    position,
                },
            );
        }
        SemanticInstruction::Store {
            destination,
            source,
        } => {
            let address = visit_memory_expr(
                destination,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            let (value, size) = match source {
                RegOrConst::Reg(GpRegister {
                    full: source,
                    slice: RegisterSliceKind::R64,
                }) => (AsmRef::Asm(RegOrConst64::Reg(source)), ValueSize::U64),
                RegOrConst::Reg(GpRegister {
                    full: source,
                    slice,
                }) => {
                    let position = BitPosition::try_from(slice).expect("U64 is handled");

                    let source = visit(
                        AsmVisitTarget::NewValue {
                            size: position.size(),
                        },
                        AsmDefinitionValue::BitCut {
                            value: AsmRef::Asm(RegOrConst64::Reg(source)),
                            position,
                        },
                    );

                    (AsmRef::Ssa(source), position.size())
                }
                RegOrConst::Const { value, size } => {
                    let Ok(size) = ValueSize::try_from(size) else {
                        unimplemented!("store for {{t,xmm,ymm,zmm}}word")
                    };

                    (AsmRef::Asm(RegOrConst64::Const(value)), size)
                }
            };

            visit(
                AsmVisitTarget::Memory,
                AsmDefinitionValue::Store {
                    address: AsmRef::from(address),
                    value,
                    size,
                },
            );
        }
        SemanticInstruction::Push { operand } => {
            // push reg16
            //  =>
            // rsp -= 8
            // slice = cut(reg64, 48..64)
            // store16(rsp, slice)

            // FIXME(hack3rmann): register size
            let (value, size) = match operand {
                Operand::Register(GpRegister {
                    full: Register64::Rsp,
                    slice,
                }) => {
                    let old_rsp = match BitPosition::try_from(slice) {
                        Ok(position) => visit(
                            AsmVisitTarget::NewValue {
                                size: position.size(),
                            },
                            AsmDefinitionValue::BitCut {
                                value: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                                position,
                            },
                        ),
                        Err(()) => visit(
                            AsmVisitTarget::NewValue {
                                size: ValueSize::U64,
                            },
                            AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Reg(
                                Register64::Rsp,
                            ))),
                        ),
                    };

                    (AsmRef::Ssa(old_rsp), ValueSize::from(slice))
                }
                Operand::Register(GpRegister { full: reg, slice }) => {
                    let value = match BitPosition::try_from(slice) {
                        Ok(position) => AsmRef::Ssa(visit(
                            AsmVisitTarget::NewValue {
                                size: position.size(),
                            },
                            AsmDefinitionValue::BitCut {
                                value: AsmRef::Asm(RegOrConst64::Reg(reg)),
                                position,
                            },
                        )),
                        Err(()) => AsmRef::Asm(RegOrConst64::Reg(reg)),
                    };

                    (value, ValueSize::from(slice))
                }
                Operand::Const(c) => (AsmRef::Asm(RegOrConst64::Const(c)), ValueSize::U64),
                Operand::Memory(expr) => {
                    let address = visit_memory_expr(
                        expr,
                        AsmVisitTarget::NewValue {
                            size: ValueSize::U64,
                        },
                        visit,
                    );
                    let value = visit(
                        // FIXME(hack3rmann): use memory size
                        AsmVisitTarget::NewValue {
                            size: ValueSize::U64,
                        },
                        // FIXME(hack3rmann): use memory size
                        AsmDefinitionValue::Load {
                            size: ValueSize::U64,
                            address: address.into(),
                        },
                    );

                    // FIXME(hack3rmann): this memory must be sized
                    (AsmRef::Ssa(value), ValueSize::U64)
                }
            };

            // new_sp = sp - 8
            let new_sp = visit(
                AsmVisitTarget::Register(Register64::Rsp),
                AsmDefinitionValue::Sub {
                    left: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                    right: AsmRef::Asm(RegOrConst64::Const(8)),
                },
            );

            visit(
                AsmVisitTarget::Memory,
                AsmDefinitionValue::Store {
                    address: AsmRef::Ssa(new_sp),
                    size,
                    value,
                },
            );
        }
        SemanticInstruction::Pop { operand, slice } => {
            match operand {
                // # OP is RSP
                // value = load16(rsp)
                // rsp = insert(rsp, value, 48..64)
                RegOrMemory::Reg(Register64::Rsp) => match BitPosition::try_from(slice) {
                    Ok(position @ BitPosition::From32To64) => {
                        let value = visit(
                            AsmVisitTarget::NewValue {
                                size: position.size(),
                            },
                            AsmDefinitionValue::Load {
                                size: position.size(),
                                address: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                            },
                        );

                        visit(
                            AsmVisitTarget::Register(Register64::Rsp),
                            AsmDefinitionValue::BitInsert {
                                destination: AsmRef::Asm(RegOrConst64::Const(0)),
                                source: AsmRef::Ssa(value),
                                position,
                            },
                        );
                    }
                    Ok(position) => {
                        let value = visit(
                            AsmVisitTarget::NewValue {
                                size: position.size(),
                            },
                            AsmDefinitionValue::Load {
                                size: position.size(),
                                address: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                            },
                        );

                        visit(
                            AsmVisitTarget::Register(Register64::Rsp),
                            AsmDefinitionValue::BitInsert {
                                destination: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                                source: AsmRef::Ssa(value),
                                position,
                            },
                        );
                    }
                    Err(()) => {
                        visit(
                            AsmVisitTarget::Register(Register64::Rsp),
                            AsmDefinitionValue::Load {
                                size: ValueSize::from(slice),
                                address: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                            },
                        );
                    }
                },
                // # OP is register and not RSP
                // value = load16(rsp)
                // reg64 = insert(reg64, value, 48..64)
                // rsp += 8
                RegOrMemory::Reg(reg) => {
                    visit(
                        AsmVisitTarget::Register(reg),
                        AsmDefinitionValue::Load {
                            // FIXME(hack3rmann): use memory size
                            size: ValueSize::U64,
                            address: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                        },
                    );

                    visit(
                        AsmVisitTarget::Register(Register64::Rsp),
                        AsmDefinitionValue::Add {
                            left: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                            right: AsmRef::Asm(RegOrConst64::Const(8)),
                        },
                    );
                }
                // # OP is memory
                // tmp = load(rsp)
                // rsp += 8
                // addr = visit_addr(op)
                // _ = store(addr, tmp)
                RegOrMemory::Mem(expr) => {
                    let loaded_tmp = visit(
                        // FIXME(hack3rmann): use memory size
                        AsmVisitTarget::NewValue {
                            size: ValueSize::U64,
                        },
                        AsmDefinitionValue::Load {
                            // FIXME(hack3rmann): use memory size
                            size: ValueSize::U64,
                            address: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                        },
                    );

                    visit(
                        AsmVisitTarget::Register(Register64::Rsp),
                        AsmDefinitionValue::Add {
                            left: AsmRef::Asm(RegOrConst64::Reg(Register64::Rsp)),
                            right: AsmRef::Asm(RegOrConst64::Const(8)),
                        },
                    );

                    let address = visit_memory_expr(
                        expr,
                        AsmVisitTarget::NewValue {
                            size: ValueSize::U64,
                        },
                        visit,
                    );

                    visit(
                        AsmVisitTarget::Memory,
                        AsmDefinitionValue::Store {
                            // FIXME(hack3rmann): use memory size
                            size: ValueSize::U64,
                            address: address.into(),
                            value: AsmRef::Ssa(loaded_tmp),
                        },
                    );
                }
            }
        }
        SemanticInstruction::LoadZeroExtend {
            destination:
                GpRegister {
                    full: destination,
                    slice: RegisterSliceKind::R32,
                },
            source,
            source_size,
        }
        | SemanticInstruction::LoadZeroExtend {
            destination:
                GpRegister {
                    full: destination,
                    slice: RegisterSliceKind::R64,
                },
            source,
            source_size,
        } => {
            // movzx reg32, any <=> movzx reg64, any
            //  =>
            // address = eval(mem)
            // value = load8(address)
            // reg64 = insert(0, value, 56..64)

            let address = visit_memory_expr(
                source,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            let Ok(size) = ValueSize::try_from(source_size) else {
                unimplemented!()
            };

            let value = visit(
                AsmVisitTarget::NewValue { size },
                AsmDefinitionValue::Load {
                    size,
                    address: address.into(),
                },
            );

            let Ok(position) = BitPosition::try_from(size) else {
                unimplemented!("movzx reg64, {source_size} value");
            };

            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Const(0)),
                    source: AsmRef::Ssa(value),
                    position,
                },
            );
        }
        SemanticInstruction::LoadZeroExtend {
            destination:
                GpRegister {
                    full: destination,
                    slice: RegisterSliceKind::R16,
                },
            source,
            source_size: PointerSize::Byte,
        } => {
            // movzx reg16, mem8
            //  =>
            // address = eval(mem)
            // value = load8(address)
            // tmp64 = insert(0, value, 56..64)
            // tmp16 = cut(tmp64, 48..64)
            // reg64 = insert(reg64, tmp16, 48..64)

            let address = visit_memory_expr(
                source,
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                visit,
            );

            let value = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U8,
                },
                AsmDefinitionValue::Load {
                    size: ValueSize::U8,
                    address: address.into(),
                },
            );

            let tmp64 = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Const(0)),
                    source: AsmRef::Ssa(value),
                    position: BitPosition::From56To64,
                },
            );

            let tmp16 = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U16,
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Ssa(tmp64),
                    position: BitPosition::From48To64,
                },
            );

            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(destination)),
                    source: AsmRef::Ssa(tmp16),
                    position: BitPosition::From48To56,
                },
            );
        }
        SemanticInstruction::LoadZeroExtend {
            destination:
                GpRegister {
                    full: _,
                    slice: RegisterSliceKind::H8 | RegisterSliceKind::L8,
                },
            source: _,
            source_size: _,
        } => {
            unimplemented!("movzx reg8, any")
        }
        SemanticInstruction::AssignmentZeroExtend {
            destination:
                GpRegister {
                    full: destination,
                    slice: RegisterSliceKind::R64,
                },
            source,
        } => {
            // movzx a64, b16
            //  =>
            // value = cut(b, 48..64)
            // a64 = insert(0, value, 48..64)

            let position = BitPosition::try_from(source.slice).expect("source is less that U64");

            let value = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::from(source.slice),
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Asm(RegOrConst64::Reg(source.full)),
                    position,
                },
            );

            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Const(0)),
                    source: AsmRef::Ssa(value),
                    position,
                },
            );
        }
        SemanticInstruction::AssignmentZeroExtend {
            destination,
            source,
        } => {
            // movzx a16, b8
            //  =>
            // tmp8 = cut(b, 56..64)
            // tmp64 = insert(0, tmp8, 56..64)
            // tmp16 = cut(tmp64, 48..64)
            // a64 = insert(a64, tmp16, 48..64)

            let destination_position =
                BitPosition::try_from(destination.slice).expect("destination is less that U64");
            let source_position =
                BitPosition::try_from(source.slice).expect("source is less that U64");

            let tmp8 = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::from(source.slice),
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Asm(RegOrConst64::Reg(source.full)),
                    position: source_position,
                },
            );

            let tmp64 = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::U64,
                },
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Const(0)),
                    source: AsmRef::Ssa(tmp8),
                    position: source_position,
                },
            );

            let tmp16 = visit(
                AsmVisitTarget::NewValue {
                    size: ValueSize::from(destination.slice),
                },
                AsmDefinitionValue::BitCut {
                    value: AsmRef::Ssa(tmp64),
                    position: destination_position,
                },
            );

            visit(
                AsmVisitTarget::Register(destination.full),
                AsmDefinitionValue::BitInsert {
                    destination: AsmRef::Asm(RegOrConst64::Reg(destination.full)),
                    source: AsmRef::Ssa(tmp16),
                    position: destination_position,
                },
            );
        }
        SemanticInstruction::Leave => {
            // leave
            //  =>
            // mov rsp, rbp
            // pop rbp

            visit_assignment(
                SemanticInstruction::Assignment {
                    slice: RegisterSliceKind::R64,
                    destination: Register64::Rsp,
                    source: RegOrConst64::Reg(Register64::Rsp),
                },
                visit,
            );

            visit_assignment(
                SemanticInstruction::Pop {
                    slice: RegisterSliceKind::R64,
                    operand: RegOrMemory::Reg(Register64::Rbp),
                },
                visit,
            );
        }
        SemanticInstruction::LoadSignExtend {
            destination: GpRegister {
                full: destination, ..
            },
            ..
        }
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
        }) => {
            visit(
                AsmVisitTarget::Register(destination),
                AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Const(42))),
            );
        }
        SemanticInstruction::Arithmetic(ArithmeticInstruction {
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
            visit(
                AsmVisitTarget::Register(first),
                AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Const(42))),
            );
            visit(
                AsmVisitTarget::Register(second),
                AsmDefinitionValue::Value(AsmRef::Asm(RegOrConst64::Const(42))),
            );
        }
        SemanticInstruction::DirectCall { .. }
        | SemanticInstruction::IndirectCallReg { .. }
        | SemanticInstruction::IndirectCallMem { .. }
        | SemanticInstruction::Return
        | SemanticInstruction::ReturnClear { .. }
        | SemanticInstruction::DirectJump { .. }
        | SemanticInstruction::IndirectJumpReg { .. }
        | SemanticInstruction::IndirectJumpMem { .. }
        | SemanticInstruction::ConditionalJump { .. }
        | SemanticInstruction::Test { .. }
        | SemanticInstruction::Cmp { .. }
        | SemanticInstruction::Nop => {}
        SemanticInstruction::Other(instruction) => {
            eprintln!("unsupported instruction {instruction:?}");
        }
        _ => {}
    }
}
