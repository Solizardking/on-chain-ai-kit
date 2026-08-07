use anyhow::Result;
use rig::agent::Agent;
use rig::providers::anthropic::completion::CompletionModel as AnthropicCompletionModel;

use crate::{
    common::{claude_agent_builder, preamble_common},
    cross_chain::tools::{ApproveToken, CheckApproval, GetQuote, Swap},
    data::{FetchCandlesticks, FetchTopTokens},
    dexscreener::tools::SearchOnDexScreener,
};

pub async fn create_cross_chain_agent(
    preamble: Option<String>,
) -> Result<Agent<AnthropicCompletionModel>> {
    let preamble = preamble.unwrap_or_else(|| {
        format!(
            "you are a cross-chain trading agent\n\n{}",
            preamble_common()
        )
    });
    Ok(claude_agent_builder()
        .preamble(&preamble)
        .tool(SearchOnDexScreener)
        .tool(GetQuote)
        .tool(Swap)
        .tool(ApproveToken)
        .tool(CheckApproval)
        .tool(FetchCandlesticks)
        .tool(FetchTopTokens)
        .build())
}
