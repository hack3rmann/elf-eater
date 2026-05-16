use bitflags::bitflags;
use iced_x86::{Formatter, Instruction, MemorySize, Mnemonic, NasmFormatter, OpKind, Register};
use std::{
    fmt::{self, Display},
    num::NonZeroU16,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstructionIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticInstruction {
    /// `call 0xWHATEVER`
    DirectCall {
        address: u64,
    },
    /// `call reg64`
    IndirectCallReg {
        register: Register64,
    },
    /// `call qword [base + index * scale + displacement]`
    IndirectCallMem {
        expr: MemoryExpression,
    },
    /// `ret`
    Return,
    /// `ret 42`
    ReturnClear {
        amount: NonZeroU16,
    },
    /// `mov reg_any, mem`
    Load {
        destination: GpRegister,
        source: MemoryExpression,
    },
    /// `movzx reg_any, mem` or `mov reg32, mem`
    LoadZeroExtend {
        destination: GpRegister,
        source: MemoryExpression,
        source_size: PointerSize,
    },
    /// `movsx reg_any, mem`
    LoadSignExtend {
        destination: GpRegister,
        source: MemoryExpression,
        source_size: PointerSize,
    },
    /// `mov mem, reg_any|const`
    Store {
        destination: MemoryExpression,
        source: RegOrConst,
    },
    /// `mov reg_any, reg_any|const`
    Assignment {
        slice: RegisterSliceKind,
        destination: Register64,
        source: RegOrConst64,
    },
    /// `movzx reg_any, reg_any` or `mov reg32, reg_any`
    AssignmentZeroExtend {
        destination: GpRegister,
        source: GpRegister,
    },
    /// `movsx reg_any, reg_any`
    AssignmentSignExtend {
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
        slice: RegisterSliceKind,
        first: RegOrMemory,
        second: RegOrMemory,
    },
    /// `jmp 0xWHATEVER`
    DirectJump {
        address: u64,
    },
    /// `jmp reg64`
    IndirectJumpReg {
        register: Register64,
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
        left: Operand,
        right: Operand,
    },
    /// `cmp left, right`
    Cmp {
        left: Operand,
        right: Operand,
    },
    /// `push op`
    Push {
        operand: Operand,
    },
    /// `pop reg64`
    Pop {
        slice: RegisterSliceKind,
        operand: RegOrMemory,
    },
    /// No-op
    Nop,
    Arithmetic(ArithmeticInstruction),
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
                destination,
                source,
            } => {
                let size = PointerSize::from(destination.slice_kind);
                write!(buf, "mov {destination}, {size} {source}").unwrap();
            }
            &SemanticInstruction::LoadZeroExtend {
                destination,
                source,
                source_size,
            } => {
                write!(buf, "movzx {destination}, {source_size} {source}").unwrap();
            }
            &SemanticInstruction::LoadSignExtend {
                destination,
                source,
                source_size,
            } => {
                write!(buf, "movsx {destination}, {source_size} {source}").unwrap();
            }
            SemanticInstruction::Store {
                destination,
                source,
            } => {
                let size = source.size();
                write!(buf, "mov {size} {destination}, {source}").unwrap();
            }
            &SemanticInstruction::Assignment {
                slice,
                destination,
                source,
            } => match source {
                RegOrConst64::Reg(source) => {
                    let destination = GpRegister::new(destination, slice);
                    let source = GpRegister::new(source, slice);

                    write!(buf, "mov {destination}, {source}").unwrap();
                }
                RegOrConst64::Const(value) => {
                    let destination = GpRegister::new(destination, slice);

                    write!(buf, "mov {destination}, {value}").unwrap();
                }
            },
            SemanticInstruction::AssignmentZeroExtend {
                destination,
                source,
            } => {
                write!(buf, "movzx {destination}, {source}").unwrap();
            }
            SemanticInstruction::AssignmentSignExtend {
                destination,
                source,
            } => {
                write!(buf, "movsx {destination}, {source}").unwrap();
            }
            SemanticInstruction::LoadAddress { destination, expr } => {
                write!(buf, "lea {destination}, {expr}").unwrap();
            }
            &SemanticInstruction::Exchange {
                slice,
                first,
                second,
            } => {
                let size = PointerSize::from(slice);

                match (first, second) {
                    (RegOrMemory::Reg(reg1), RegOrMemory::Reg(reg2)) => {
                        let first = GpRegister::new(reg1, slice);
                        let second = GpRegister::new(reg2, slice);

                        write!(buf, "xchg {first}, {second}").unwrap();
                    }
                    (RegOrMemory::Reg(reg), RegOrMemory::Mem(mem))
                    | (RegOrMemory::Mem(mem), RegOrMemory::Reg(reg)) => {
                        let reg = GpRegister::new(reg, slice);

                        write!(buf, "xchg {reg}, {size} {mem}").unwrap();
                    }
                    (RegOrMemory::Mem(mem1), RegOrMemory::Mem(mem2)) => {
                        // NOTE: unreachable actually
                        write!(buf, "xchg {size} {mem1}, {size} {mem2}").unwrap();
                    }
                }
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
            SemanticInstruction::Test { left, right } => {
                write!(buf, "test {left}, {right}").unwrap();
            }
            SemanticInstruction::Cmp { left, right } => {
                write!(buf, "cmp {left}, {right}").unwrap();
            }
            SemanticInstruction::Push { operand } => {
                write!(buf, "push {operand}").unwrap();
            }
            &SemanticInstruction::Pop { slice, operand } => match operand {
                RegOrMemory::Reg(reg64) => {
                    let reg = GpRegister::new(reg64, slice);
                    write!(buf, "pop {reg}").unwrap();
                }
                RegOrMemory::Mem(mem) => {
                    write!(buf, "pop {mem}").unwrap();
                }
            },
            SemanticInstruction::Arithmetic(instruction) => {
                write!(buf, "{instruction}").unwrap();
            }
            SemanticInstruction::Nop => buf.push_str("nop"),
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
            .or_else(|| lift_movzx(instr))
            .or_else(|| lift_movsx(instr))
            .or_else(|| lift_lea(instr))
            .or_else(|| lift_xchg(instr))
            .or_else(|| lift_push(instr))
            .or_else(|| lift_pop(instr))
            .or_else(|| lift_jump(instr))
            .or_else(|| lift_jcc(instr))
            .or_else(|| lift_test(instr))
            .or_else(|| lift_cmp(instr))
            .or_else(|| lift_arithmetic(instr))
            .or_else(|| lift_nop(instr))
            .unwrap_or(SemanticInstruction::Other(instr))
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Flags: u8 {
        const ZERO = 1 << 0;
        const CARRY = 1 << 1;
        const SIGN = 1 << 2;
        const OVERFLOW = 1 << 3;
        const PARITY = 1 << 4;
        const AUXILLIARY_OVERFLOW = 1 << 5;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlagsEffect {
    pub read: Flags,
    pub written: Flags,
    pub undefined: Flags,
    pub set_mask: Flags,
    pub set_values: Flags,
}

impl FlagsEffect {
    pub const NONE: Self = Self {
        read: Flags::empty(),
        written: Flags::empty(),
        undefined: Flags::empty(),
        set_mask: Flags::empty(),
        set_values: Flags::empty(),
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtendedBinaryOpKind {
    #[default]
    Imul,
    Mul,
    Div,
    Idiv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegOrMemory {
    Reg(Register64),
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
pub enum SizedRegOrMemory {
    Reg(GpRegister),
    Mem {
        size: PointerSize,
        expr: MemoryExpression,
    },
}

impl SizedRegOrMemory {
    pub fn new(reg_or_mem: RegOrMemory, slice: RegisterSliceKind) -> Self {
        match reg_or_mem {
            RegOrMemory::Reg(reg) => Self::Reg(GpRegister::new(reg, slice)),
            RegOrMemory::Mem(expr) => Self::Mem {
                size: PointerSize::from(slice),
                expr,
            },
        }
    }
}

impl Display for SizedRegOrMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SizedRegOrMemory::Reg(gp_register) => gp_register.fmt(f),
            SizedRegOrMemory::Mem { size, expr } => write!(f, "{size} {expr}"),
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

impl From<SizedRegOrMemory> for Operand {
    fn from(value: SizedRegOrMemory) -> Self {
        match value {
            SizedRegOrMemory::Reg(reg) => Self::Register(reg),
            SizedRegOrMemory::Mem { size: _, expr } => Self::Memory(expr),
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

impl From<RegisterSliceKind> for PointerSize {
    fn from(value: RegisterSliceKind) -> Self {
        match value {
            RegisterSliceKind::R64 => Self::Qword,
            RegisterSliceKind::R32 => Self::Dword,
            RegisterSliceKind::R16 => Self::Word,
            RegisterSliceKind::H8 | RegisterSliceKind::L8 => Self::Byte,
        }
    }
}

impl TryFrom<MemorySize> for PointerSize {
    type Error = ();

    fn try_from(value: MemorySize) -> Result<Self, Self::Error> {
        Ok(match value.size() {
            1 => Self::Byte,
            2 => Self::Word,
            4 => Self::Dword,
            8 => Self::Qword,
            10 => Self::Tword,
            16 => Self::XmmWord,
            32 => Self::YmmWord,
            64 => Self::ZmmWord,
            _ => return Err(()),
        })
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
        base: Option<Register64>,
        index: Option<Register64>,
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
    pub base: Option<Register64>,
    pub index: Option<Register64>,
    pub scale: MemoryScale,
    pub displacement: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeMemoryExpression {
    pub size: Option<PointerSize>,
    pub address: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtendedRegister {
    pub upper: Register64,
    pub lower: Register64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegOrConst {
    Reg(GpRegister),
    Const { value: u64, size: PointerSize },
}

impl RegOrConst {
    pub fn size(self) -> PointerSize {
        match self {
            RegOrConst::Reg(reg) => reg.slice_kind.into(),
            RegOrConst::Const { value: _, size } => size,
        }
    }
}

impl Display for RegOrConst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegOrConst::Reg(register) => register.fmt(f),
            RegOrConst::Const { value, size: _ } => value.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegOrConst64 {
    Reg(Register64),
    Const(u64),
}

impl Display for RegOrConst64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegOrConst64::Reg(reg) => reg.fmt(f),
            RegOrConst64::Const(c) => c.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GpRegister {
    pub full: Register64,
    pub slice_kind: RegisterSliceKind,
}

impl GpRegister {
    pub const fn new(full: Register64, slice_kind: RegisterSliceKind) -> Self {
        Self { full, slice_kind }
    }

    pub const fn with_slice(self, slice_kind: RegisterSliceKind) -> Self {
        Self::new(self.full, slice_kind)
    }

    pub const fn as_str(self) -> &'static str {
        match (self.full, self.slice_kind) {
            (Register64::Rax, RegisterSliceKind::R64) => "reg_a",
            (Register64::Rax, RegisterSliceKind::R32) => "reg_a[32..]",
            (Register64::Rax, RegisterSliceKind::R16) => "reg_a[48..]",
            (Register64::Rax, RegisterSliceKind::H8) => "reg_a[48..56]",
            (Register64::Rax, RegisterSliceKind::L8) => "reg_a[56..]",
            (Register64::Rbx, RegisterSliceKind::R64) => "reg_b",
            (Register64::Rbx, RegisterSliceKind::R32) => "reg_b[32..]",
            (Register64::Rbx, RegisterSliceKind::R16) => "reg_b[48..]",
            (Register64::Rbx, RegisterSliceKind::H8) => "reg_b[48..56]",
            (Register64::Rbx, RegisterSliceKind::L8) => "reg_b[56..]",
            (Register64::Rcx, RegisterSliceKind::R64) => "reg_c",
            (Register64::Rcx, RegisterSliceKind::R32) => "reg_c[32..]",
            (Register64::Rcx, RegisterSliceKind::R16) => "reg_c[48..]",
            (Register64::Rcx, RegisterSliceKind::H8) => "reg_c[48..56]",
            (Register64::Rcx, RegisterSliceKind::L8) => "reg_c[56..]",
            (Register64::Rdx, RegisterSliceKind::R64) => "reg_d",
            (Register64::Rdx, RegisterSliceKind::R32) => "reg_d[32..]",
            (Register64::Rdx, RegisterSliceKind::R16) => "reg_d[48..]",
            (Register64::Rdx, RegisterSliceKind::H8) => "reg_d[48..56]",
            (Register64::Rdx, RegisterSliceKind::L8) => "reg_d[56..]",
            (Register64::Rsi, RegisterSliceKind::R64) => "reg_si",
            (Register64::Rsi, RegisterSliceKind::R32) => "reg_si[32..]",
            (Register64::Rsi, RegisterSliceKind::R16) => "reg_si[48..]",
            (Register64::Rsi, RegisterSliceKind::H8) => "reg_si[48..56]",
            (Register64::Rsi, RegisterSliceKind::L8) => "reg_si[56..]",
            (Register64::Rdi, RegisterSliceKind::R64) => "reg_di",
            (Register64::Rdi, RegisterSliceKind::R32) => "reg_di[32..]",
            (Register64::Rdi, RegisterSliceKind::R16) => "reg_di[48..]",
            (Register64::Rdi, RegisterSliceKind::H8) => "reg_di[48..56]",
            (Register64::Rdi, RegisterSliceKind::L8) => "reg_di[56..]",
            (Register64::Rbp, RegisterSliceKind::R64) => "reg_bp",
            (Register64::Rbp, RegisterSliceKind::R32) => "reg_bp[32..]",
            (Register64::Rbp, RegisterSliceKind::R16) => "reg_bp[48..]",
            (Register64::Rbp, RegisterSliceKind::H8) => "reg_bp[48..56]",
            (Register64::Rbp, RegisterSliceKind::L8) => "reg_bp[56..]",
            (Register64::Rsp, RegisterSliceKind::R64) => "reg_sp",
            (Register64::Rsp, RegisterSliceKind::R32) => "reg_sp[32..]",
            (Register64::Rsp, RegisterSliceKind::R16) => "reg_sp[48..]",
            (Register64::Rsp, RegisterSliceKind::H8) => "reg_sp[48..56]",
            (Register64::Rsp, RegisterSliceKind::L8) => "reg_sp[56..]",
            (Register64::R8, RegisterSliceKind::R64) => "reg_8",
            (Register64::R8, RegisterSliceKind::R32) => "reg_8[32..]",
            (Register64::R8, RegisterSliceKind::R16) => "reg_8[48..]",
            (Register64::R8, RegisterSliceKind::H8) => "reg_8[48..56]",
            (Register64::R8, RegisterSliceKind::L8) => "reg_8[56..]",
            (Register64::R9, RegisterSliceKind::R64) => "reg_9",
            (Register64::R9, RegisterSliceKind::R32) => "reg_9[32..]",
            (Register64::R9, RegisterSliceKind::R16) => "reg_9[48..]",
            (Register64::R9, RegisterSliceKind::H8) => "reg_9[48..56]",
            (Register64::R9, RegisterSliceKind::L8) => "reg_9[56..]",
            (Register64::R10, RegisterSliceKind::R64) => "reg_10",
            (Register64::R10, RegisterSliceKind::R32) => "reg_10[32..]",
            (Register64::R10, RegisterSliceKind::R16) => "reg_10[48..]",
            (Register64::R10, RegisterSliceKind::H8) => "reg_10[48..56]",
            (Register64::R10, RegisterSliceKind::L8) => "reg_10[56..]",
            (Register64::R11, RegisterSliceKind::R64) => "reg_11",
            (Register64::R11, RegisterSliceKind::R32) => "reg_11[32..]",
            (Register64::R11, RegisterSliceKind::R16) => "reg_11[48..]",
            (Register64::R11, RegisterSliceKind::H8) => "reg_11[48..56]",
            (Register64::R11, RegisterSliceKind::L8) => "reg_11[56..]",
            (Register64::R12, RegisterSliceKind::R64) => "reg_12",
            (Register64::R12, RegisterSliceKind::R32) => "reg_12[32..]",
            (Register64::R12, RegisterSliceKind::R16) => "reg_12[48..]",
            (Register64::R12, RegisterSliceKind::H8) => "reg_12[48..56]",
            (Register64::R12, RegisterSliceKind::L8) => "reg_12[56..]",
            (Register64::R13, RegisterSliceKind::R64) => "reg_13",
            (Register64::R13, RegisterSliceKind::R32) => "reg_13[32..]",
            (Register64::R13, RegisterSliceKind::R16) => "reg_13[48..]",
            (Register64::R13, RegisterSliceKind::H8) => "reg_13[48..56]",
            (Register64::R13, RegisterSliceKind::L8) => "reg_13[56..]",
            (Register64::R14, RegisterSliceKind::R64) => "reg_14",
            (Register64::R14, RegisterSliceKind::R32) => "reg_14[32..]",
            (Register64::R14, RegisterSliceKind::R16) => "reg_14[48..]",
            (Register64::R14, RegisterSliceKind::H8) => "reg_14[48..56]",
            (Register64::R14, RegisterSliceKind::L8) => "reg_14[56..]",
            (Register64::R15, RegisterSliceKind::R64) => "reg_15",
            (Register64::R15, RegisterSliceKind::R32) => "reg_15[32..]",
            (Register64::R15, RegisterSliceKind::R16) => "reg_15[48..]",
            (Register64::R15, RegisterSliceKind::H8) => "reg_15[48..56]",
            (Register64::R15, RegisterSliceKind::L8) => "reg_15[56..]",
        }
    }
}

impl TryFrom<Register> for GpRegister {
    type Error = ();

    fn try_from(value: Register) -> Result<Self, Self::Error> {
        Ok(Self {
            full: Register64::try_from(value)?,
            slice_kind: RegisterSliceKind::try_from(value)?,
        })
    }
}

impl Display for GpRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Register64 {
    #[default]
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Register64 {
    pub const fn as_str(self) -> &'static str {
        GpRegister::new(self, RegisterSliceKind::R64).as_str()
    }
}

impl Display for Register64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<Register> for Register64 {
    type Error = ();

    fn try_from(reg: Register) -> Result<Self, Self::Error> {
        Ok(match reg.full_register() {
            Register::RAX => Self::Rax,
            Register::RBX => Self::Rbx,
            Register::RCX => Self::Rcx,
            Register::RDX => Self::Rdx,
            Register::RSI => Self::Rsi,
            Register::RDI => Self::Rdi,
            Register::RBP => Self::Rbp,
            Register::RSP => Self::Rsp,
            Register::R8 => Self::R8,
            Register::R9 => Self::R9,
            Register::R10 => Self::R10,
            Register::R11 => Self::R11,
            Register::R12 => Self::R12,
            Register::R13 => Self::R13,
            Register::R14 => Self::R14,
            Register::R15 => Self::R15,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegisterSliceKind {
    /// 64-bit full register
    #[default]
    R64,
    /// Low 32 bits
    R32,
    /// Low 16 bits
    R16,
    /// High 8 bits of low 16 bits
    H8,
    /// Low 8 bits of low 16 bits
    L8,
}

impl TryFrom<Register> for RegisterSliceKind {
    type Error = ();

    fn try_from(value: Register) -> Result<Self, Self::Error> {
        Ok(match value {
            Register::SPL
            | Register::BPL
            | Register::SIL
            | Register::DIL
            | Register::R8L
            | Register::R9L
            | Register::R10L
            | Register::R11L
            | Register::R12L
            | Register::R13L
            | Register::R14L
            | Register::R15L
            | Register::AL
            | Register::CL
            | Register::DL
            | Register::BL => Self::L8,
            Register::AH | Register::CH | Register::DH | Register::BH => Self::H8,
            Register::AX
            | Register::CX
            | Register::DX
            | Register::BX
            | Register::SP
            | Register::BP
            | Register::SI
            | Register::DI
            | Register::R8W
            | Register::R9W
            | Register::R10W
            | Register::R11W
            | Register::R12W
            | Register::R13W
            | Register::R14W
            | Register::R15W => Self::R16,
            Register::EAX
            | Register::ECX
            | Register::EDX
            | Register::EBX
            | Register::ESP
            | Register::EBP
            | Register::ESI
            | Register::EDI
            | Register::EIP
            | Register::R8D
            | Register::R9D
            | Register::R10D
            | Register::R11D
            | Register::R12D
            | Register::R13D
            | Register::R14D
            | Register::R15D => Self::R32,
            Register::RAX
            | Register::RCX
            | Register::RDX
            | Register::RBX
            | Register::RSP
            | Register::RBP
            | Register::RSI
            | Register::RDI
            | Register::R8
            | Register::R9
            | Register::R10
            | Register::R11
            | Register::R12
            | Register::R13
            | Register::R14
            | Register::R15
            | Register::RIP => Self::R64,
            _ => return Err(()),
        })
    }
}

impl TryFrom<MemorySize> for RegisterSliceKind {
    type Error = ();

    fn try_from(value: MemorySize) -> Result<Self, Self::Error> {
        Ok(match value.size() {
            1 => Self::L8,
            2 => Self::R16,
            4 => Self::R32,
            8 => Self::R64,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArithmeticInstruction {
    pub operands: ArithmeticOperands,
    pub flags_effect: FlagsEffect,
    pub kind: ArithmeticOpKind,
}

impl Display for ArithmeticInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = self.kind.as_str();

        match self.operands {
            ArithmeticOperands::TernaryExpression {
                result,
                first,
                second,
                third,
            } => {
                write!(f, "{result} = {op}({first}, {second}, {third})")
            }
            ArithmeticOperands::ShortExpression {
                result,
                left,
                right: Some(right),
            } => {
                write!(f, "{result} = {left} {op} {right}")
            }
            ArithmeticOperands::ShortExpression {
                result,
                left,
                right: None,
            } => {
                write!(f, "{result} = {op}{left}")
            }
            ArithmeticOperands::ResultExtendedExpression {
                result_hi,
                result_lo,
                left,
                right,
            } => {
                write!(f, "({result_hi}, {result_lo}) = {left} {op} {right}")
            }
            ArithmeticOperands::OperandExtendedExpression {
                result,
                left_hi,
                left_lo,
                right_hi,
                right_lo,
            } => {
                write!(
                    f,
                    "{result} = ({left_hi}, {left_lo}) {op} ({right_hi}, {right_lo})"
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArithmeticOperands {
    ShortExpression {
        result: SizedRegOrMemory,
        left: Operand,
        right: Option<Operand>,
    },
    TernaryExpression {
        result: SizedRegOrMemory,
        first: Operand,
        second: Operand,
        third: Operand,
    },
    ResultExtendedExpression {
        result_hi: SizedRegOrMemory,
        result_lo: SizedRegOrMemory,
        left: Operand,
        right: Operand,
    },
    OperandExtendedExpression {
        result: SizedRegOrMemory,
        left_hi: Operand,
        left_lo: Operand,
        right_hi: Operand,
        right_lo: Operand,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArithmeticOpKind {
    #[default]
    Add,
    Sub,
    And,
    Or,
    Xor,
    Not,
    ShiftLeft,
    ShiftRight,
    ShiftArithmeticRight,
    RotateLeft,
    RotateRight,
    RotateWithCarryLeft,
    RotateWithCarryRight,
    ShiftPreciseLeft,
    ShiftPreciseRight,
}

impl ArithmeticOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::And => "&",
            Self::Or => "|",
            Self::Xor => "^",
            Self::Not => "!",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::ShiftArithmeticRight => ">>s",
            Self::RotateLeft => "<<r",
            Self::RotateRight => ">>r",
            Self::RotateWithCarryLeft => "<<rc",
            Self::RotateWithCarryRight => ">>rc",
            Self::ShiftPreciseLeft => "shift_left_from",
            Self::ShiftPreciseRight => "shift_right_from",
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
        r => Some(Register64::try_from(r).ok()?),
    };

    let index = match instr.memory_index() {
        Register::None => None,
        r => Some(Register64::try_from(r).ok()?),
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
            let reg = Register64::try_from(instr.op0_register()).ok()?;
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

    Some(if let Some(amount) = NonZeroU16::new(instr.immediate16()) {
        SemanticInstruction::ReturnClear { amount }
    } else {
        SemanticInstruction::Return
    })
}

fn lift_mov(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Mov {
        return None;
    }

    Some(match (instr.op0_kind(), instr.op1_kind()) {
        (OpKind::Register, OpKind::Memory) => {
            let destination = GpRegister::try_from(instr.op0_register()).ok()?;
            let source = lift_memory(&instr)?;

            // Zero extending 32-bit mov
            if destination.slice_kind == RegisterSliceKind::R32 {
                SemanticInstruction::LoadZeroExtend {
                    destination: destination.with_slice(RegisterSliceKind::R64),
                    source,
                    source_size: PointerSize::Dword,
                }
            } else {
                SemanticInstruction::Load {
                    destination,
                    source,
                }
            }
        }
        (
            OpKind::Memory,
            OpKind::Register
            | OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8_2nd
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64,
        ) => {
            let size = PointerSize::try_from(instr.memory_size()).ok()?;

            SemanticInstruction::Store {
                destination: lift_memory(&instr)?,
                source: lift_reg_or_const(&instr, 1, size)?,
            }
        }
        (OpKind::Register, OpKind::Register) => {
            let slice = RegisterSliceKind::try_from(instr.op0_register()).ok()?;
            let destination = Register64::try_from(instr.op0_register()).ok()?;
            let source = Register64::try_from(instr.op1_register()).ok()?;

            // Zero extending 32-bit mov
            if slice == RegisterSliceKind::R32 {
                SemanticInstruction::AssignmentZeroExtend {
                    destination: GpRegister::new(destination, RegisterSliceKind::R64),
                    source: GpRegister::new(source, RegisterSliceKind::R32),
                }
            } else {
                SemanticInstruction::Assignment {
                    slice,
                    destination,
                    source: RegOrConst64::Reg(source),
                }
            }
        }
        (
            OpKind::Register,
            OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8_2nd
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64,
        ) => SemanticInstruction::Assignment {
            slice: RegisterSliceKind::try_from(instr.op0_register()).ok()?,
            destination: Register64::try_from(instr.op0_register()).ok()?,
            source: RegOrConst64::Const(instr.immediate64()),
        },
        _ => return None,
    })
}

fn lift_reg_or_mem(instr: &Instruction, op: u32) -> Option<RegOrMemory> {
    Some(match instr.op_kind(op) {
        OpKind::Register => RegOrMemory::Reg(Register64::try_from(instr.op_register(op)).ok()?),
        OpKind::Memory => RegOrMemory::Mem(lift_memory(instr)?),
        _ => return None,
    })
}

fn lift_reg_or_const(instr: &Instruction, op: u32, size: PointerSize) -> Option<RegOrConst> {
    Some(match instr.op_kind(op) {
        OpKind::Register => RegOrConst::Reg(GpRegister::try_from(instr.op_register(op)).ok()?),
        OpKind::Immediate8
        | OpKind::Immediate16
        | OpKind::Immediate32
        | OpKind::Immediate64
        | OpKind::Immediate8_2nd
        | OpKind::Immediate8to16
        | OpKind::Immediate8to32
        | OpKind::Immediate8to64
        | OpKind::Immediate32to64 => RegOrConst::Const {
            value: instr.immediate64(),
            size,
        },
        _ => return None,
    })
}

fn lift_operand(instr: &Instruction, op: u32) -> Option<Operand> {
    Some(match instr.op_kind(op) {
        OpKind::Register => Operand::Register(GpRegister::try_from(instr.op_register(op)).ok()?),
        OpKind::Memory => Operand::Memory(lift_memory(instr)?),
        OpKind::Immediate8
        | OpKind::Immediate16
        | OpKind::Immediate32
        | OpKind::Immediate64
        | OpKind::Immediate8_2nd
        | OpKind::Immediate8to16
        | OpKind::Immediate8to32
        | OpKind::Immediate8to64
        | OpKind::Immediate32to64 => Operand::Const(instr.immediate64()),
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
            register: Register64::try_from(instr.op0_register()).ok()?,
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

    let slice = match (instr.op0_kind(), instr.op1_kind()) {
        (OpKind::Register, _) => RegisterSliceKind::try_from(instr.op0_register()).ok()?,
        (_, OpKind::Register) => RegisterSliceKind::try_from(instr.op1_register()).ok()?,
        _ => RegisterSliceKind::R64,
    };

    Some(SemanticInstruction::Exchange {
        slice,
        first: lift_reg_or_mem(&instr, 0)?,
        second: lift_reg_or_mem(&instr, 1)?,
    })
}

fn lift_push(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Push {
        return None;
    }

    Some(SemanticInstruction::Push {
        operand: lift_operand(&instr, 0)?,
    })
}

fn lift_pop(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Pop {
        return None;
    }

    let slice = match instr.op0_kind() {
        OpKind::Register => RegisterSliceKind::try_from(instr.op0_register()).ok()?,
        OpKind::Memory => RegisterSliceKind::R64,
        _ => return None,
    };

    Some(SemanticInstruction::Pop {
        slice,
        operand: lift_reg_or_mem(&instr, 0)?,
    })
}

fn lift_cmp(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Cmp {
        return None;
    }

    Some(SemanticInstruction::Cmp {
        left: lift_operand(&instr, 0)?,
        right: lift_operand(&instr, 1)?,
    })
}

fn lift_test(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Test {
        return None;
    }

    Some(SemanticInstruction::Test {
        left: lift_operand(&instr, 0)?,
        right: lift_operand(&instr, 1)?,
    })
}

fn lift_movzx(instr: Instruction) -> Option<SemanticInstruction> {
    if instr.mnemonic() != Mnemonic::Movzx {
        return None;
    }

    let destination = GpRegister::try_from(instr.op0_register()).ok()?;

    Some(match instr.op1_kind() {
        OpKind::Register => SemanticInstruction::AssignmentZeroExtend {
            destination,
            source: GpRegister::try_from(instr.op1_register()).ok()?,
        },
        OpKind::Memory => SemanticInstruction::LoadZeroExtend {
            destination,
            source: lift_memory(&instr)?,
            source_size: PointerSize::try_from(instr.memory_size()).ok()?,
        },
        _ => return None,
    })
}

fn lift_movsx(instr: Instruction) -> Option<SemanticInstruction> {
    if !matches!(instr.mnemonic(), Mnemonic::Movsx | Mnemonic::Movsxd) {
        return None;
    }

    let destination = GpRegister::try_from(instr.op0_register()).ok()?;

    Some(match instr.op1_kind() {
        OpKind::Register => SemanticInstruction::AssignmentSignExtend {
            destination,
            source: GpRegister::try_from(instr.op1_register()).ok()?,
        },
        OpKind::Memory => SemanticInstruction::LoadSignExtend {
            destination,
            source: lift_memory(&instr)?,
            source_size: PointerSize::try_from(instr.memory_size()).ok()?,
        },
        _ => return None,
    })
}

fn lift_arithmetic(instr: Instruction) -> Option<SemanticInstruction> {
    lift_add_sub(instr)
        .or_else(|| lift_inc_dec(instr))
        .or_else(|| lift_and_or_xor(instr))
        .or_else(|| lift_not(instr))
        .or_else(|| lift_shl_shr_sal_sar(instr))
        .or_else(|| lift_rol_ror_rcl_rcr(instr))
        .or_else(|| lift_shld_shrd(instr))
        .or_else(|| lift_neg(instr))
        .map(SemanticInstruction::Arithmetic)
}

fn lift_sized_mem_or_reg_from_2ops(
    instr: &Instruction,
    operand_index: u32,
) -> Option<SizedRegOrMemory> {
    let result_unsized = lift_reg_or_mem(instr, operand_index)?;

    let slice = match (instr.op0_kind(), instr.op1_kind()) {
        (OpKind::Register, kind) if kind != OpKind::Memory => {
            RegisterSliceKind::try_from(instr.op0_register()).ok()?
        }
        (kind, OpKind::Register) if kind != OpKind::Memory => {
            RegisterSliceKind::try_from(instr.op1_register()).ok()?
        }
        (OpKind::Memory, _) | (_, OpKind::Memory) => {
            RegisterSliceKind::try_from(instr.memory_size()).ok()?
        }
        _ => return None,
    };

    Some(SizedRegOrMemory::new(result_unsized, slice))
}

fn lift_short_arith_expression(instr: &Instruction) -> Option<ArithmeticOperands> {
    Some(ArithmeticOperands::ShortExpression {
        result: lift_sized_mem_or_reg_from_2ops(instr, 0)?,
        left: lift_operand(instr, 0)?,
        right: Some(lift_operand(instr, 1)?),
    })
}

fn lift_add_sub(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::Add => ArithmeticOpKind::Add,
        Mnemonic::Sub => ArithmeticOpKind::Sub,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: lift_short_arith_expression(&instr)?,
        flags_effect: FlagsEffect {
            written: Flags::ZERO
                | Flags::CARRY
                | Flags::SIGN
                | Flags::OVERFLOW
                | Flags::PARITY
                | Flags::AUXILLIARY_OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind,
    })
}

fn lift_neg(instr: Instruction) -> Option<ArithmeticInstruction> {
    if instr.mnemonic() != Mnemonic::Neg {
        return None;
    }

    let result = match instr.op0_kind() {
        OpKind::Register => SizedRegOrMemory::Reg(GpRegister::try_from(instr.op0_register()).ok()?),
        OpKind::Memory => SizedRegOrMemory::Mem {
            size: PointerSize::try_from(instr.memory_size()).ok()?,
            expr: lift_memory(&instr)?,
        },
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: ArithmeticOperands::ShortExpression {
            result,
            left: Operand::Const(0),
            right: Some(Operand::from(result)),
        },
        flags_effect: FlagsEffect {
            written: Flags::ZERO
                | Flags::CARRY
                | Flags::SIGN
                | Flags::OVERFLOW
                | Flags::PARITY
                | Flags::AUXILLIARY_OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind: ArithmeticOpKind::Sub,
    })
}

fn lift_inc_dec(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::Inc => ArithmeticOpKind::Add,
        Mnemonic::Dec => ArithmeticOpKind::Sub,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: ArithmeticOperands::ShortExpression {
            result: lift_sized_mem_or_reg_from_2ops(&instr, 0)?,
            left: lift_operand(&instr, 0)?,
            right: Some(Operand::Const(1)),
        },
        flags_effect: FlagsEffect {
            written: Flags::ZERO
                | Flags::SIGN
                | Flags::OVERFLOW
                | Flags::PARITY
                | Flags::AUXILLIARY_OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind,
    })
}

fn lift_and_or_xor(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::And => ArithmeticOpKind::And,
        Mnemonic::Or => ArithmeticOpKind::Or,
        Mnemonic::Xor => ArithmeticOpKind::Xor,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: lift_short_arith_expression(&instr)?,
        flags_effect: FlagsEffect {
            read: Flags::empty(),
            written: Flags::CARRY | Flags::OVERFLOW | Flags::ZERO | Flags::SIGN | Flags::PARITY,
            undefined: Flags::AUXILLIARY_OVERFLOW,
            set_mask: Flags::CARRY | Flags::OVERFLOW,
            set_values: Flags::empty(),
        },
        kind,
    })
}

fn lift_not(instr: Instruction) -> Option<ArithmeticInstruction> {
    if instr.mnemonic() != Mnemonic::Not {
        return None;
    };

    Some(ArithmeticInstruction {
        operands: ArithmeticOperands::ShortExpression {
            result: lift_sized_mem_or_reg_from_2ops(&instr, 0)?,
            left: lift_operand(&instr, 0)?,
            right: None,
        },
        flags_effect: FlagsEffect::NONE,
        kind: ArithmeticOpKind::Not,
    })
}

fn lift_shl_shr_sal_sar(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::Shl | Mnemonic::Sal => ArithmeticOpKind::ShiftLeft,
        Mnemonic::Shr => ArithmeticOpKind::ShiftRight,
        Mnemonic::Sar => ArithmeticOpKind::ShiftArithmeticRight,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: lift_short_arith_expression(&instr)?,
        flags_effect: FlagsEffect {
            written: Flags::CARRY | Flags::OVERFLOW | Flags::ZERO | Flags::SIGN | Flags::PARITY,
            undefined: Flags::AUXILLIARY_OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind,
    })
}

fn lift_rol_ror_rcl_rcr(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::Rol => ArithmeticOpKind::RotateLeft,
        Mnemonic::Ror => ArithmeticOpKind::RotateRight,
        Mnemonic::Rcl => ArithmeticOpKind::RotateWithCarryLeft,
        Mnemonic::Rcr => ArithmeticOpKind::RotateWithCarryRight,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: lift_short_arith_expression(&instr)?,
        flags_effect: FlagsEffect {
            written: Flags::CARRY | Flags::OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind,
    })
}

fn lift_shld_shrd(instr: Instruction) -> Option<ArithmeticInstruction> {
    let kind = match instr.mnemonic() {
        Mnemonic::Shld => ArithmeticOpKind::ShiftPreciseLeft,
        Mnemonic::Shrd => ArithmeticOpKind::ShiftPreciseRight,
        _ => return None,
    };

    Some(ArithmeticInstruction {
        operands: ArithmeticOperands::TernaryExpression {
            // NOTE(hack3rmann): 2 ops to check is fine, because the third is always an immediate
            result: lift_sized_mem_or_reg_from_2ops(&instr, 0)?,
            first: lift_operand(&instr, 0)?,
            second: lift_operand(&instr, 1)?,
            third: lift_operand(&instr, 2)?,
        },
        flags_effect: FlagsEffect {
            written: Flags::CARRY | Flags::OVERFLOW | Flags::ZERO | Flags::SIGN | Flags::PARITY,
            undefined: Flags::AUXILLIARY_OVERFLOW,
            ..FlagsEffect::NONE
        },
        kind,
    })
}

fn lift_nop(instr: Instruction) -> Option<SemanticInstruction> {
    (instr.mnemonic() == Mnemonic::Nop).then_some(SemanticInstruction::Nop)
}
