use crate::asm::passes::{
    code_flow::{BlockIndex, FunctionCodeFlow},
    references::FunctionInfo,
    semantic::{ConditionalType, SemanticInstruction},
};
use bitflags::bitflags;
use smallvec::{SmallVec, smallvec};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExplicitInstruction {
    /// `branch rflags.contains(flags_checked), on_true, on_false`
    RflagsBranch {
        flags_checked: ModifiedFlags,
        on_true: BlockIndex,
        on_false: BlockIndex,
    },
    /// Semantically the same a the explicit
    SameSemantic(SemanticInstruction),
    /// Other (probably implicit)
    Other(SemanticInstruction),
}

impl ExplicitInstruction {
    pub fn new(
        code_flow: &FunctionCodeFlow,
        info: &FunctionInfo,
        instruction: SemanticInstruction,
    ) -> SmallVec<[Self; 4]> {
        match instruction {
            // TODO(hack3rmann): actually create the variants
            SemanticInstruction::DirectCall { .. }
            | SemanticInstruction::IndirectCallReg { .. }
            | SemanticInstruction::IndirectCallMem { .. }
            | SemanticInstruction::Return
            | SemanticInstruction::ReturnClear { .. }
            | SemanticInstruction::Load { .. }
            | SemanticInstruction::Store { .. }
            | SemanticInstruction::Assignment { .. }
            | SemanticInstruction::LoadAddress { .. }
            | SemanticInstruction::Exchange { .. }
            | SemanticInstruction::DirectJump { .. }
            | SemanticInstruction::IndirectJumpReg { .. }
            | SemanticInstruction::IndirectJumpMem { .. } => {
                smallvec![Self::SameSemantic(instruction)]
            }
            SemanticInstruction::Other(_) => smallvec![Self::Other(instruction)],

            SemanticInstruction::ConditionalJump { address, ty } => {
                let instruction_index = info.address_map[&address] as u32;
                let on_true = code_flow.index_to_block[&instruction_index];
                let on_false = code_flow.index_to_block[&(instruction_index + 1)];

                let result = Self::RflagsBranch {
                    flags_checked: ModifiedFlags::from(ty),
                    on_true,
                    on_false,
                };

                smallvec![result]
            }

            SemanticInstruction::Test { size, left, right } => todo!(),
            SemanticInstruction::Cmp { size, left, right } => todo!(),
            SemanticInstruction::PushConst { value } => todo!(),
            SemanticInstruction::PushReg { reg } => todo!(),
            SemanticInstruction::PushMem { expr } => todo!(),
            SemanticInstruction::PopReg { reg } => todo!(),
            SemanticInstruction::PopMem { expr } => todo!(),
            SemanticInstruction::BinaryOp {
                kind,
                destination,
                left,
                right,
            } => todo!(),
            SemanticInstruction::UnaryOp { kind, operand } => todo!(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlagState {
    #[default]
    Unchecked = 0,
    Set = 1,
    Unset = 2,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Flags: u8 {
        const CARRY = 1 << 0;
        const ZERO = 1 << 1;
        const SIGN = 1 << 2;
        const OVERFLOW = 1 << 3;
        const PARITY = 1 << 4;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModifiedFlags {
    pub mask: Flags,
    pub expected_one_of: SmallVec<[Flags; 8]>,
}

impl From<ConditionalType> for ModifiedFlags {
    fn from(value: ConditionalType) -> Self {
        match value {
            ConditionalType::Equal => Self {
                mask: Flags::ZERO,
                expected_one_of: smallvec![Flags::ZERO],
            },
            ConditionalType::NotEqual => Self {
                mask: Flags::ZERO,
                expected_one_of: smallvec![Flags::empty()],
            },
            ConditionalType::Below => Self {
                mask: Flags::CARRY,
                expected_one_of: smallvec![Flags::CARRY],
            },
            ConditionalType::AboveOrEqual => Self {
                mask: Flags::CARRY,
                expected_one_of: smallvec![Flags::empty()],
            },
            ConditionalType::Above => Self {
                mask: Flags::CARRY | Flags::ZERO,
                expected_one_of: smallvec![Flags::empty()],
            },
            ConditionalType::BelowOrEqual => Self {
                mask: Flags::CARRY | Flags::ZERO,
                expected_one_of: smallvec![Flags::CARRY | Flags::ZERO],
            },
            ConditionalType::Less => Self {
                mask: Flags::SIGN | Flags::OVERFLOW,
                expected_one_of: smallvec![Flags::SIGN, Flags::OVERFLOW],
            },
            ConditionalType::GreaterOrEqaual => Self {
                mask: Flags::SIGN | Flags::OVERFLOW,
                expected_one_of: smallvec![Flags::SIGN | Flags::OVERFLOW, Flags::empty()],
            },
            ConditionalType::Greater => Self {
                mask: Flags::ZERO | Flags::SIGN | Flags::OVERFLOW,
                expected_one_of: smallvec![Flags::SIGN | Flags::OVERFLOW, Flags::empty()],
            },
            ConditionalType::LessOrEqual => Self {
                mask: Flags::ZERO | Flags::SIGN | Flags::OVERFLOW,
                expected_one_of: smallvec![
                    Flags::OVERFLOW,
                    Flags::SIGN,
                    Flags::ZERO,
                    Flags::ZERO | Flags::OVERFLOW,
                    Flags::ZERO | Flags::SIGN,
                    Flags::ZERO | Flags::SIGN | Flags::OVERFLOW,
                ],
            },
            ConditionalType::Overflow => Self {
                mask: Flags::OVERFLOW,
                expected_one_of: smallvec![Flags::OVERFLOW],
            },
            ConditionalType::NoOverflow => Self {
                mask: Flags::OVERFLOW,
                expected_one_of: smallvec![Flags::empty()],
            },
            ConditionalType::Negative => Self {
                mask: Flags::SIGN,
                expected_one_of: smallvec![Flags::SIGN],
            },
            ConditionalType::NonNegative => Self {
                mask: Flags::SIGN,
                expected_one_of: smallvec![Flags::empty()],
            },
            ConditionalType::ParityEven => Self {
                mask: Flags::PARITY,
                expected_one_of: smallvec![Flags::PARITY],
            },
            ConditionalType::ParityOdd => Self {
                mask: Flags::PARITY,
                expected_one_of: smallvec![Flags::empty()],
            },
        }
    }
}
