use iced_x86::{Formatter, Instruction, Mnemonic, NasmFormatter, OpKind, Register};
use std::fmt::{self, Display};

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
        size: PointerSize,
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
        address: u64,
        ty: ConditionalType,
    },
    /// `test left, right`
    Test {
        size: PointerSize,
        left: Operand,
        right: Operand,
    },
    /// `cmp left, right`
    Cmp {
        size: PointerSize,
        left: Operand,
        right: Operand,
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
    Other(Instruction),
}

impl SemanticInstruction {
    pub fn format(&self, formatter: &mut NasmFormatter, buf: &mut String) {
        use std::fmt::Write;

        match self {
            SemanticInstruction::DirectCall { address } => {
                write!(buf, "call 0x{address:x}").unwrap();
            }
            SemanticInstruction::IndirectCallReg { register } => {
                write!(buf, "call {register}").unwrap();
            }
            SemanticInstruction::IndirectCallMem { expr } => {
                write!(buf, "call qword {expr}").unwrap();
            }
            SemanticInstruction::Return => {
                buf.write_str("ret").unwrap();
            }
            SemanticInstruction::ReturnClear { amount } => {
                write!(buf, "ret 0x{amount:x}").unwrap();
            }
            SemanticInstruction::Load {
                size,
                destination,
                source,
            } => {
                write!(buf, "mov {destination}, {size} {source}").unwrap();
            }
            SemanticInstruction::Store {
                size,
                destination,
                source,
            } => {
                write!(buf, "mov {size} {destination}, {source}").unwrap();
            }
            SemanticInstruction::Assignment {
                size: _,
                destination,
                source,
            } => {
                // FIXME(hack3rmann): handle size
                write!(buf, "mov {destination}, {source}").unwrap();
            }
            SemanticInstruction::LoadAddress { destination, expr } => {
                write!(buf, "lea {destination}, {expr}").unwrap();
            }
            SemanticInstruction::Exchange {
                size: _,
                first,
                second,
            } => {
                // FIXME(hack3rmann): handle size
                write!(buf, "xchg {first}, {second}").unwrap();
            }
            SemanticInstruction::DirectJump { address } => {
                write!(buf, "jmp 0x{address:x}").unwrap();
            }
            SemanticInstruction::IndirectJumpReg { register } => {
                write!(buf, "jmp {register}").unwrap();
            }
            SemanticInstruction::IndirectJumpMem { expr } => {
                write!(buf, "jmp qword {expr}").unwrap();
            }
            SemanticInstruction::ConditionalJump { address, ty } => {
                write!(buf, "j{ty} 0x{address:x}").unwrap();
            }
            SemanticInstruction::Test {
                size: _,
                left,
                right,
            } => {
                // TODO(hack3rmann): register sizes
                write!(buf, "test {left}, {right}").unwrap();
            }
            SemanticInstruction::Cmp {
                size: _,
                left,
                right,
            } => {
                // TODO(hack3rmann): register sizes
                write!(buf, "cmp {left}, {right}").unwrap();
            }
            SemanticInstruction::PushConst { value } => {
                write!(buf, "push {value}").unwrap();
            }
            SemanticInstruction::PushReg { reg } => {
                write!(buf, "push {reg}").unwrap();
            }
            SemanticInstruction::PushMem { expr } => {
                write!(buf, "push qword {expr}").unwrap();
            }
            SemanticInstruction::PopReg { reg } => {
                write!(buf, "pop {reg}").unwrap();
            }
            SemanticInstruction::PopMem { expr } => {
                write!(buf, "pop {expr}").unwrap();
            }
            SemanticInstruction::BinaryOp {
                kind,
                destination,
                left,
                right,
            } => {
                write!(buf, "{kind} {destination}, {left}, {right}").unwrap();
            }
            SemanticInstruction::UnaryOp { kind, operand } => {
                write!(buf, "{kind} {operand}").unwrap();
            }
            SemanticInstruction::Other(instruction) => {
                formatter.format(instruction, buf);
            }
        }
    }
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
            .or_else(|| lift_test(instr))
            .or_else(|| lift_cmp(instr))
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

impl BinaryOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            BinaryOpKind::Add => "add",
            BinaryOpKind::Sub => "sub",
            BinaryOpKind::Mul => "mul",
            BinaryOpKind::Imul => "imul",
            BinaryOpKind::Xor => "xor",
            BinaryOpKind::And => "and",
            BinaryOpKind::Or => "or",
            BinaryOpKind::Shl => "shl",
            BinaryOpKind::Shr => "shr",
        }
    }
}

impl Display for BinaryOpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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

impl UnaryOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            UnaryOpKind::Neg => "neg",
            UnaryOpKind::Inv => "inv",
            UnaryOpKind::Inc => "inc",
            UnaryOpKind::Dec => "dec",
        }
    }
}

impl Display for UnaryOpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegOrMemory {
    Reg(GpRegister),
    Mem(MemoryExpression),
}

impl Display for RegOrMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegOrMemory::Reg(gp_register) => gp_register.fmt(f),
            RegOrMemory::Mem(memory_expression) => memory_expression.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operand {
    Register(GpRegister),
    Memory(MemoryExpression),
    Const(u64),
}

impl Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Register(gp_register) => gp_register.fmt(f),
            Operand::Memory(memory_expression) => memory_expression.fmt(f),
            Operand::Const(value) => write!(f, "0x{value:x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalType {
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

impl ConditionalType {
    pub const fn as_str(self) -> &'static str {
        match self {
            ConditionalType::Equal => "e",
            ConditionalType::NotEqual => "ne",
            ConditionalType::Below => "b",
            ConditionalType::BelowOrEqual => "be",
            ConditionalType::Above => "a",
            ConditionalType::AboveOrEqual => "ae",
            ConditionalType::Less => "l",
            ConditionalType::LessOrEqual => "le",
            ConditionalType::Greater => "g",
            ConditionalType::GreaterOrEqaual => "ge",
            ConditionalType::Overflow => "o",
            ConditionalType::NoOverflow => "no",
            ConditionalType::Negative => "s",
            ConditionalType::NonNegative => "ns",
            ConditionalType::ParityEven => "p",
            ConditionalType::ParityOdd => "np",
        }
    }
}

impl Display for ConditionalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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

impl PointerSize {
    pub fn as_str(self) -> &'static str {
        match self {
            PointerSize::Byte => "byte",
            PointerSize::Word => "word",
            PointerSize::Dword => "dword",
            PointerSize::Qword => "qword",
            PointerSize::Tword => "tword",
            PointerSize::XmmWord => "xmmword",
            PointerSize::YmmWord => "ymmword",
            PointerSize::ZmmWord => "zmmword",
        }
    }
}

impl Display for PointerSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryScale {
    #[default]
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

impl MemoryScale {
    pub const fn as_str(self) -> &'static str {
        match self {
            MemoryScale::One => "1",
            MemoryScale::Two => "2",
            MemoryScale::Four => "4",
            MemoryScale::Eight => "8",
        }
    }

    pub const fn value(self) -> u8 {
        self as u8
    }
}

impl Display for MemoryScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryExpression {
    /// `[base + index * scale + displacement]`
    Absolute {
        base: Option<GpRegister>,
        index: Option<GpRegister>,
        scale: MemoryScale,
        displacement: i64,
    },
    /// `[rip + displacement]` or `size [rel displacement]`, where `address = rip + displacement`
    Relative { address: u64 },
}

impl Display for MemoryExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let format_displacement = |f: &mut fmt::Formatter<'_>, displacement: i64| match displacement
        {
            0 => f.write_str("]"),
            ..0 => write!(f, " - 0x{:x}]", (-displacement) as u64),
            1.. => write!(f, " + 0x{displacement:x}]"),
        };

        match self {
            MemoryExpression::Absolute {
                base: Some(base),
                index: Some(index),
                scale,
                displacement,
            } => {
                if *scale == MemoryScale::One {
                    write!(f, "[{base} + {index}")?;
                } else {
                    write!(f, "[{base} + {index} * {scale}")?;
                }

                format_displacement(f, *displacement)
            }
            MemoryExpression::Absolute {
                base: None,
                index: Some(index),
                scale,
                displacement,
            } => {
                if *scale == MemoryScale::One {
                    write!(f, "[{index}")?;
                } else {
                    write!(f, "[{index} * {scale}")?;
                }

                format_displacement(f, *displacement)
            }
            MemoryExpression::Absolute {
                base: Some(base),
                index: None,
                scale: _,
                displacement,
            } => {
                write!(f, "[{base}")?;
                format_displacement(f, *displacement)
            }
            MemoryExpression::Absolute {
                base: None,
                index: None,
                scale: _,
                displacement: address,
            } => {
                write!(f, "[0x{address:x}]")
            }
            MemoryExpression::Relative { address } => {
                write!(f, "[rel 0x{address:x} - rip]")
            }
        }
    }
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
            address: value.address,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsoluteMemoryExpression {
    pub base: Option<GpRegister>,
    pub index: Option<GpRegister>,
    pub scale: MemoryScale,
    pub displacement: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeMemoryExpression {
    pub size: Option<PointerSize>,
    pub address: u64,
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

impl GpRegister {
    pub const fn as_str(self) -> &'static str {
        match self {
            GpRegister::Rax => "rax",
            GpRegister::Rbx => "rbx",
            GpRegister::Rcx => "rcx",
            GpRegister::Rdx => "rdx",
            GpRegister::Rsi => "rsi",
            GpRegister::Rdi => "rdi",
            GpRegister::Rbp => "rbp",
            GpRegister::Rsp => "rsp",
            GpRegister::R8 => "r8",
            GpRegister::R9 => "r9",
            GpRegister::R10 => "r10",
            GpRegister::R11 => "r11",
            GpRegister::R12 => "r12",
            GpRegister::R13 => "r13",
            GpRegister::R14 => "r14",
            GpRegister::R15 => "r15",
        }
    }
}

impl Display for GpRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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

fn lift_memory(instr: &Instruction) -> Option<MemoryExpression> {
    if instr.memory_base() == Register::RIP {
        return Some(MemoryExpression::Relative {
            address: instr.ip_rel_memory_address(),
        });
    }

    let base = match instr.memory_base() {
        Register::None => None,
        r => Some(GpRegister::try_from(r).ok()?),
    };

    let index = match instr.memory_index() {
        Register::None => None,
        r => Some(GpRegister::try_from(r).ok()?),
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

fn lift_jcc_type(mnemonic: Mnemonic) -> Option<ConditionalType> {
    use ConditionalType::*;
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
        address: instr.near_branch_target(),
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
        // FIXME(hack3rmann): xchg size
        size: PointerSize::Qword,
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

fn lift_cmp(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Cmp {
        return None;
    }

    Some(SemanticInstruction::Cmp {
        size: PointerSize::Qword,
        left: lift_operand(&instr, 0)?,
        right: lift_operand(&instr, 1)?,
    })
}

fn lift_test(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Test {
        return None;
    }

    Some(SemanticInstruction::Test {
        size: PointerSize::Qword,
        left: lift_operand(&instr, 0)?,
        right: lift_operand(&instr, 1)?,
    })
}
