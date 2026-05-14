use iced_x86::Instruction;

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
    /// `mov destination, source`
    Load {
        size: PointerSize,
        destination: GpRegister,
        source: MemoryExpression,
    },
    /// `mov destination, source`
    Store {
        size: PointerSize,
        destination: MemoryExpression,
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
        kind: BinaryOpKind,
        destination: GpRegister,
        left: RegOrMemory,
    },
    UnaryOp {
        kind: UnaryOpKind,
    },
    /// CFG-form end of a block
    BlockTerminator,
    Other(Instruction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryOpKind {
    Add,
    Sub,
    Mul,
    Xor,
    And,
    Or,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryOpKind {
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
        base: GpRegister,
        index: GpNotRspRegister,
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
    pub base: GpRegister,
    pub index: GpNotRspRegister,
    pub scale: MemoryScale,
    pub displacement: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeMemoryExpression {
    pub size: Option<PointerSize>,
    pub displacement: i64,
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
