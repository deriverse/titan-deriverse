pub mod instruction_constants {

    pub trait DrvInstruction {
        const INSTRUCTION_NUMBER: u8;
        const MIN_ACCOUNTS: usize;
    }

    pub struct NewInstrumentInstruction;
    impl DrvInstruction for NewInstrumentInstruction {
        const INSTRUCTION_NUMBER: u8 = 9;
        const MIN_ACCOUNTS: usize = 19;
    }

    pub struct SwapInstruction;
    impl DrvInstruction for SwapInstruction {
        const INSTRUCTION_NUMBER: u8 = 26;
        const MIN_ACCOUNTS: usize = 14;
    }
}
