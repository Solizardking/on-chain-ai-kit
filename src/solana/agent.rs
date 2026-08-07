use anyhow::Result;
use rig::agent::Agent;
use crate::mesh::MeshCompletionModel;

use super::tools::{
    BuyPumpFunToken, DeployPumpFunToken, FetchTokenPrice, GetPortfolio, GetPublicKey,
    GetSolBalance, GetSplTokenBalance, PerformJupiterSwap, SellPumpFunToken, TransferSol,
    TransferSplToken,
};
use crate::common::{mesh_agent_builder, preamble_common};
use crate::dexscreener::tools::SearchOnDexScreener;

pub async fn create_solana_agent(
    preamble: Option<String>,
) -> Result<Agent<MeshCompletionModel>> {
    let preamble = preamble.unwrap_or_else(|| {
        format!(
            "you are a solana trading agent that can also interact with pump.fun;\n\n{}",
            preamble_common()
        )
    });
    Ok(mesh_agent_builder()
        .preamble(&preamble)
        .max_tokens(1024)
        .tool(PerformJupiterSwap)
        .tool(TransferSol)
        .tool(TransferSplToken)
        .tool(GetPublicKey)
        .tool(GetSolBalance)
        .tool(GetSplTokenBalance)
        .tool(FetchTokenPrice)
        .tool(GetPortfolio)
        .tool(SearchOnDexScreener)
        .tool(DeployPumpFunToken)
        .tool(BuyPumpFunToken)
        .tool(SellPumpFunToken)
        .build())
}
