use iced_x86::{Instruction, Mnemonic, OpKind, Register};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticInstruction {
    /// `call 0xWHATEVER`
    DirectCall {
        address: u64,
    },
    /// `call reg64`
    IndirectCallReg {
        register: GpRegister,
    },
    /// `call qword [base + index * scale + displacement]`
    IndirectCallMem {
        expr: MemoryExpression,
    },
    /// `ret`
    Return,
    /// `ret 42`
    ReturnClear {
        amount: u16,
    },
    /// `mov reg_any, mem`
    Load {
        size: PointerSize,
        destination: GpRegister,
        source: MemoryExpression,
    },
    /// `mov mem, reg_any`
    Store {
        size: PointerSize,
        destination: MemoryExpression,
        source: GpRegister,
    },
    /// `mov reg_any, reg_any`
    Assignment {
        size: PointerSize,
        destination: GpRegister,
        source: GpRegister,
    },
    /// `lea reg64, [expr]`
    LoadAddress {
        destination: GpRegister,
        expr: MemoryExpression,
    },
    /// `xchg destination, source`
    Exchange {
        first: RegOrMemory,
        second: RegOrMemory,
    },
    /// `jmp 0xWHATEVER`
    DirectJump {
        address: u64,
    },
    /// `jmp reg64`
    IndirectJumpReg {
        register: GpRegister,
    },
    /// `jmp qword [base + index * scale + displacement]`
    IndirectJumpMem {
        expr: MemoryExpression,
    },
    /// `jcc 0xWHATEVER`
    ConditionalJump {
        address: i64,
        ty: ConditionalJumpType,
    },
    /// `push 42`
    PushConst {
        value: u64,
    },
    /// `push reg64`
    PushReg {
        reg: GpRegister,
    },
    /// `push qword [mem]`
    PushMem {
        expr: MemoryExpression,
    },
    /// `pop reg64`
    PopReg {
        reg: GpRegister,
    },
    /// `pop qword [mem]`
    PopMem {
        expr: MemoryExpression,
    },
    BinaryOp {
        // TODO(hack3rmann): operand sizes
        kind: BinaryOpKind,
        destination: RegOrMemory,
        left: Operand,
        right: Operand,
    },
    UnaryOp {
        kind: UnaryOpKind,
        operand: RegOrMemory,
    },
    /// CFG-form end of a block
    BlockTerminator,
    Other(Instruction),
}

impl From<Instruction> for SemanticInstruction {
    fn from(instr: Instruction) -> Self {
        lift_call(instr)
            .or_else(|| lift_ret(instr))
            .or_else(|| lift_mov(instr))
            .or_else(|| lift_lea(instr))
            .or_else(|| lift_xchg(instr))
            .or_else(|| lift_push(instr))
            .or_else(|| lift_pop(instr))
            .or_else(|| lift_jump(instr))
            .or_else(|| lift_jcc(instr))
            .or_else(|| lift_binary_op(instr))
            .unwrap_or(SemanticInstruction::Other(instr))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryOpKind {
    #[default]
    Add,
    Sub,
    Mul,
    Imul,
    Xor,
    And,
    Or,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtendedBinaryOpKind {
    #[default]
    Imul,
    Mul,
    Div,
    Idiv,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryOpKind {
    #[default]
    Neg,
    Inv,
    Inc,
    Dec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegOrMemory {
    Reg(GpRegister),
    Mem(MemoryExpression),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operand {
    Register(GpRegister),
    Memory(MemoryExpression),
    Const(u64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalJumpType {
    #[default]
    Equal,
    NotEqual,
    Below,
    BelowOrEqual,
    Above,
    AboveOrEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqaual,
    Overflow,
    NoOverflow,
    Negative,
    NonNegative,
    ParityEven,
    ParityOdd,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PointerSize {
    Byte = 1,
    Word = 2,
    Dword = 4,
    #[default]
    Qword = 8,
    Tword = 10,
    XmmWord = 16,
    YmmWord = 32,
    ZmmWord = 64,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryScale {
    #[default]
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryExpression {
    /// `[base + index * scale + displacement]`
    Absolute {
        base: Option<GpRegister>,
        index: Option<GpNotRspRegister>,
        scale: MemoryScale,
        displacement: i64,
    },
    /// `[rip + displacement]` or `size [rel displacement]`
    Relative { displacement: i64 },
}

impl From<AbsoluteMemoryExpression> for MemoryExpression {
    fn from(value: AbsoluteMemoryExpression) -> Self {
        Self::Absolute {
            base: value.base,
            index: value.index,
            scale: value.scale,
            displacement: value.displacement,
        }
    }
}

impl From<RelativeMemoryExpression> for MemoryExpression {
    fn from(value: RelativeMemoryExpression) -> Self {
        Self::Relative {
            displacement: value.displacement,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsoluteMemoryExpression {
    pub base: Option<GpRegister>,
    pub index: Option<GpNotRspRegister>,
    pub scale: MemoryScale,
    pub displacement: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeMemoryExpression {
    pub size: Option<PointerSize>,
    pub displacement: i64,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtendedGpRegister {
    pub upper: GpRegister,
    pub lower: GpRegister,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpRegister {
    #[default]
    Rax = 0,
    Rbx = 1,
    Rcx = 2,
    Rdx = 3,
    Rsi = 4,
    Rdi = 5,
    Rbp = 6,
    Rsp = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl TryFrom<Register> for GpRegister {
    type Error = ();

    fn try_from(reg: Register) -> Result<Self, Self::Error> {
        match reg.full_register() {
            Register::RAX => Ok(Self::Rax),
            Register::RBX => Ok(Self::Rbx),
            Register::RCX => Ok(Self::Rcx),
            Register::RDX => Ok(Self::Rdx),
            Register::RSI => Ok(Self::Rsi),
            Register::RDI => Ok(Self::Rdi),
            Register::RBP => Ok(Self::Rbp),
            Register::RSP => Ok(Self::Rsp),
            Register::R8 => Ok(Self::R8),
            Register::R9 => Ok(Self::R9),
            Register::R10 => Ok(Self::R10),
            Register::R11 => Ok(Self::R11),
            Register::R12 => Ok(Self::R12),
            Register::R13 => Ok(Self::R13),
            Register::R14 => Ok(Self::R14),
            Register::R15 => Ok(Self::R15),
            _ => Err(()),
        }
    }
}

impl From<GpNotRspRegister> for GpRegister {
    fn from(value: GpNotRspRegister) -> Self {
        match value {
            GpNotRspRegister::Rax => Self::Rax,
            GpNotRspRegister::Rbx => Self::Rbx,
            GpNotRspRegister::Rcx => Self::Rcx,
            GpNotRspRegister::Rdx => Self::Rdx,
            GpNotRspRegister::Rsi => Self::Rsi,
            GpNotRspRegister::Rdi => Self::Rdi,
            GpNotRspRegister::Rbp => Self::Rbp,
            GpNotRspRegister::R8 => Self::R8,
            GpNotRspRegister::R9 => Self::R9,
            GpNotRspRegister::R10 => Self::R10,
            GpNotRspRegister::R11 => Self::R11,
            GpNotRspRegister::R12 => Self::R12,
            GpNotRspRegister::R13 => Self::R13,
            GpNotRspRegister::R14 => Self::R14,
            GpNotRspRegister::R15 => Self::R15,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpNotRspRegister {
    #[default]
    Rax = 0,
    Rbx = 1,
    Rcx = 2,
    Rdx = 3,
    Rsi = 4,
    Rdi = 5,
    Rbp = 6,
    // No Rsp
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl TryFrom<Register> for GpNotRspRegister {
    type Error = ();

    fn try_from(reg: Register) -> Result<Self, Self::Error> {
        match reg.full_register() {
            Register::RAX => Ok(Self::Rax),
            Register::RBX => Ok(Self::Rbx),
            Register::RCX => Ok(Self::Rcx),
            Register::RDX => Ok(Self::Rdx),
            Register::RSI => Ok(Self::Rsi),
            Register::RDI => Ok(Self::Rdi),
            Register::RBP => Ok(Self::Rbp),
            Register::R8 => Ok(Self::R8),
            Register::R9 => Ok(Self::R9),
            Register::R10 => Ok(Self::R10),
            Register::R11 => Ok(Self::R11),
            Register::R12 => Ok(Self::R12),
            Register::R13 => Ok(Self::R13),
            Register::R14 => Ok(Self::R14),
            Register::R15 => Ok(Self::R15),
            _ => Err(()),
        }
    }
}

fn lift_memory(instr: &Instruction) -> Option<MemoryExpression> {
    if instr.memory_base() == Register::RIP {
        return Some(MemoryExpression::Relative {
            displacement: instr.memory_displacement64() as i64,
        });
    }

    let base = match instr.memory_base() {
        Register::None => None,
        r => Some(GpRegister::try_from(r).ok()?),
    };

    let index = match instr.memory_index() {
        Register::None => None,
        r => Some(GpNotRspRegister::try_from(r).ok()?),
    };

    let scale = match instr.memory_index_scale() {
        1 => MemoryScale::One,
        2 => MemoryScale::Two,
        4 => MemoryScale::Four,
        8 => MemoryScale::Eight,
        _ => return None,
    };

    Some(MemoryExpression::Absolute {
        base,
        index,
        scale,
        displacement: instr.memory_displacement64() as i64,
    })
}

fn lift_call(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.is_call_near() {
        return Some(SemanticInstruction::DirectCall {
            address: instr.near_branch_target(),
        });
    }

    if !instr.is_call_near_indirect() {
        return None;
    }

    Some(match instr.op0_kind() {
        OpKind::Register => {
            let reg = GpRegister::try_from(instr.op0_register()).ok()?;
            SemanticInstruction::IndirectCallReg { register: reg }
        }
        OpKind::Memory => SemanticInstruction::IndirectCallMem {
            expr: lift_memory(&instr)?,
        },
        _ => return None,
    })
}

fn lift_ret(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Ret {
        return None;
    }

    Some(if instr.immediate16() == 0 {
        SemanticInstruction::Return
    } else {
        SemanticInstruction::ReturnClear {
            amount: instr.immediate16(),
        }
    })
}

fn lift_mov(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Mov {
        return None;
    }

    Some(match (instr.op0_kind(), instr.op1_kind()) {
        (OpKind::Register, OpKind::Memory) => {
            SemanticInstruction::Load {
                // FIXME(hack3rmann): pointer size
                size: PointerSize::Qword,
                destination: GpRegister::try_from(instr.op0_register()).ok()?,
                source: lift_memory(&instr)?,
            }
        }
        (OpKind::Memory, OpKind::Register) => {
            SemanticInstruction::Store {
                // FIXME(hack3rmann): pointer size
                size: PointerSize::Qword,
                destination: lift_memory(&instr)?,
                source: GpRegister::try_from(instr.op1_register()).ok()?,
            }
        }
        (OpKind::Register, OpKind::Register) => {
            SemanticInstruction::Assignment {
                // FIXME(hack3rmann): pointer size
                size: PointerSize::Qword,
                destination: GpRegister::try_from(instr.op0_register()).ok()?,
                source: GpRegister::try_from(instr.op1_register()).ok()?,
            }
        }
        _ => return None,
    })
}

fn lift_reg_or_mem(instr: &Instruction, op: u32) -> Option<RegOrMemory> {
    Some(match instr.op_kind(op) {
        OpKind::Register => RegOrMemory::Reg(GpRegister::try_from(instr.op_register(op)).ok()?),
        OpKind::Memory => RegOrMemory::Mem(lift_memory(instr)?),
        _ => return None,
    })
}

fn lift_operand(instr: &Instruction, op: u32) -> Option<Operand> {
    Some(match instr.op_kind(op) {
        OpKind::Register => Operand::Register(GpRegister::try_from(instr.op_register(op)).ok()?),
        OpKind::Memory => Operand::Memory(lift_memory(instr)?),
        OpKind::Immediate8 | OpKind::Immediate16 | OpKind::Immediate32 | OpKind::Immediate64 => {
            Operand::Const(instr.immediate64())
        }
        _ => return None,
    })
}

fn lift_jump(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.is_jmp_short_or_near() {
        return Some(SemanticInstruction::DirectJump {
            address: instr.near_branch_target(),
        });
    }

    if !instr.is_jmp_near_indirect() {
        return None;
    }

    Some(match instr.op0_kind() {
        OpKind::Register => SemanticInstruction::IndirectJumpReg {
            register: GpRegister::try_from(instr.op0_register()).ok()?,
        },
        OpKind::Memory => SemanticInstruction::IndirectJumpMem {
            expr: lift_memory(&instr)?,
        },
        _ => return None,
    })
}

fn lift_jcc_type(mnemonic: Mnemonic) -> Option<ConditionalJumpType> {
    use ConditionalJumpType::*;
    use iced_x86::Mnemonic::*;

    Some(match mnemonic {
        Je => Equal,
        Jne => NotEqual,
        Jb => Below,
        Jbe => BelowOrEqual,
        Ja => Above,
        Jae => AboveOrEqual,
        Jl => Less,
        Jle => LessOrEqual,
        Jg => Greater,
        Jge => GreaterOrEqaual,
        Jo => Overflow,
        Jno => NoOverflow,
        Js => Negative,
        Jns => NonNegative,
        Jp => ParityEven,
        Jnp => ParityOdd,
        _ => return None,
    })
}

fn lift_jcc(instr: Instruction) -> Option<SemanticInstruction> {
    Some(SemanticInstruction::ConditionalJump {
        address: instr.near_branch_target() as i64,
        ty: lift_jcc_type(instr.mnemonic())?,
    })
}

fn lift_lea(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Lea {
        return None;
    }

    Some(SemanticInstruction::LoadAddress {
        destination: GpRegister::try_from(instr.op0_register()).ok()?,
        expr: lift_memory(&instr)?,
    })
}

fn lift_xchg(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Xchg {
        return None;
    }

    Some(SemanticInstruction::Exchange {
        first: lift_reg_or_mem(&instr, 0)?,
        second: lift_reg_or_mem(&instr, 1)?,
    })
}

fn lift_push(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Push {
        return None;
    }

    Some(match instr.op0_kind() {
        OpKind::Immediate8 | OpKind::Immediate16 | OpKind::Immediate32 | OpKind::Immediate64 => {
            SemanticInstruction::PushConst {
                value: instr.immediate64(),
            }
        }
        OpKind::Register => SemanticInstruction::PushReg {
            reg: GpRegister::try_from(instr.op0_register()).ok()?,
        },
        OpKind::Memory => SemanticInstruction::PushMem {
            expr: lift_memory(&instr)?,
        },
        _ => return None,
    })
}

fn lift_pop(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Pop {
        return None;
    }

    Some(match instr.op0_kind() {
        OpKind::Register => SemanticInstruction::PopReg {
            reg: GpRegister::try_from(instr.op0_register()).ok()?,
        },
        OpKind::Memory => SemanticInstruction::PopMem {
            expr: lift_memory(&instr)?,
        },
        _ => return None,
    })
}

fn lift_binop_kind(mnemonic: Mnemonic) -> Option<BinaryOpKind> {
    use BinaryOpKind::*;

    Some(match mnemonic {
        Mnemonic::Add => Add,
        Mnemonic::Sub => Sub,
        Mnemonic::Imul => Imul,
        Mnemonic::Mul => Mul,
        Mnemonic::Xor => Xor,
        Mnemonic::And => And,
        Mnemonic::Or => Or,
        Mnemonic::Shl => Shl,
        Mnemonic::Shr => Shr,
        _ => return None,
    })
}

fn lift_binary_op(instr: Instruction) -> Option<SemanticInstruction> {
    Some(SemanticInstruction::BinaryOp {
        kind: lift_binop_kind(instr.mnemonic())?,
        destination: lift_reg_or_mem(&instr, 0)?,
        left: lift_operand(&instr, 1)?,
        right: lift_operand(&instr, 2)?,
    })
}
