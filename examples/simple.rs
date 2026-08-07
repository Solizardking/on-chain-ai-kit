#[cfg(feature = "solana")]
use {
    anyhow::Result,
    openclawd_solana_kit::common::mesh_agent_builder,
    openclawd_solana_kit::signer::solana::LocalSolanaSigner,
    openclawd_solana_kit::signer::SignerContext,
    openclawd_solana_kit::solana::util::env,
    rig::completion::Prompt,
    std::sync::Arc,
};

#[cfg(feature = "solana")]
#[tokio::main]
async fn main() -> Result<()> {
    use openclawd_solana_kit::solana::tools::GetPortfolio;

    let signer = LocalSolanaSigner::new(env("SOLANA_PRIVATE_KEY"));
    SignerContext::with_signer(Arc::new(signer), async {
        // Defaults to Clawd inference mesh (OpenAI-compatible)
        let agent = mesh_agent_builder()
            .preamble("you are a portfolio checker; call tools when needed and explain briefly")
            .max_tokens(1024)
            .tool(GetPortfolio)
            .build();

        let response = agent
            .prompt("whats the portfolio looking like?")
            .await?;
        println!("{response}");

        Ok(())
    })
    .await?;

    Ok(())
}

#[cfg(not(feature = "solana"))]
fn main() {
    println!("enable the solana feature to run this example");
}
