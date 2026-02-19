use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;

pub trait BuildContext {}

pub trait Context
where
    Self: Sized,
{
    type Build: BuildContext;

    fn build(
        rpc: &RpcClient,
        build_ctx: Self::Build,
    ) -> Result<Box<Self>, solana_client::client_error::ClientError>;

    fn create_instruction(&self) -> Instruction;
}

pub trait InstructionBuilder {
    fn new_builder<U: Context>(
        &self,
        ctx: U::Build,
    ) -> Result<Box<U>, solana_client::client_error::ClientError>;
}
