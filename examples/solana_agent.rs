#[cfg(feature = "solana")]
use {
    anyhow::Result, openclawd_solana_kit::reasoning_loop::ReasoningLoop,
    openclawd_solana_kit::signer::solana::LocalSolanaSigner,
    openclawd_solana_kit::signer::SignerContext,
    openclawd_solana_kit::solana::agent::create_solana_agent,
    openclawd_solana_kit::solana::util::env, rig::completion::Message, rig::message::UserContent,
    rig::OneOrMany, std::sync::Arc,
};

#[cfg(feature = "solana")]
#[tokio::main]
async fn main() -> Result<()> {
    let signer = LocalSolanaSigner::new(env("SOLANA_PRIVATE_KEY"));

    SignerContext::with_signer(Arc::new(signer), async {
        let trader_agent = Arc::new(create_solana_agent(None).await?);
        let wrapped_agent = ReasoningLoop::new(trader_agent).with_stdout(true);

        wrapped_agent
            .stream(
                vec![Message::User {
                    content: OneOrMany::one(UserContent::text(
                        "what is the liquidity in the pool for my largest holding?",
                    )),
                }],
                None,
            )
            .await?;

        Ok(())
    })
    .await?;

    Ok(())
}

#[cfg(not(feature = "solana"))]
fn main() {
    println!("enable the 'solana' feature to run this example.");
}
