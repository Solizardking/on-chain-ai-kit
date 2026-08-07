use anyhow::Result;
use rig::agent::Agent;
use crate::mesh::MeshCompletionModel;

use super::tools::{
    ApproveTokenForRouterSpend, GetErc20Balance, GetEthBalance, Trade, TransferErc20, TransferEth,
    VerifySwapRouterHasAllowance, WalletAddress,
};
use crate::common::{mesh_agent_builder, preamble_common};

pub async fn create_evm_agent(preamble: Option<String>) -> Result<Agent<MeshCompletionModel>> {
    let preamble = preamble.unwrap_or_else(|| {
        format!(
            "you are an ethereum trading agent\n\n{}",
            preamble_common()
        )
    });
    Ok(mesh_agent_builder()
        .preamble(&preamble)
        .max_tokens(1024)
        .tool(Trade)
        .tool(TransferEth)
        .tool(TransferErc20)
        .tool(WalletAddress)
        .tool(GetEthBalance)
        .tool(GetErc20Balance)
        .tool(ApproveTokenForRouterSpend)
        .tool(VerifySwapRouterHasAllowance)
        .build())
}
