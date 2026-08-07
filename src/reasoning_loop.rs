//! Multi-turn tool loop for kit agents.
//!
//! Uses non-streaming completion so OpenAI-compatible backends (Clawd mesh)
//! work. Chunks are still emitted over SSE as Message / ToolCall events.

use anyhow::Result;
use rig::agent::Agent;
use rig::completion::{AssistantContent, Message};
use rig::message::{ToolResultContent, UserContent};
use rig::completion::Completion;
use crate::mesh::MeshCompletionModel as CompletionModel;
use rig::OneOrMany;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

pub enum LoopResponse {
    Message(String),
    ToolCall { name: String, result: String },
}

pub struct ReasoningLoop {
    agent: Arc<Agent<CompletionModel>>,
    stdout: bool,
}

impl ReasoningLoop {
    pub fn new(agent: Arc<Agent<CompletionModel>>) -> Self {
        Self {
            agent,
            stdout: true,
        }
    }

    pub fn with_stdout(mut self, enabled: bool) -> Self {
        self.stdout = enabled;
        self
    }

    pub async fn stream(
        &self,
        messages: Vec<Message>,
        tx: Option<Sender<LoopResponse>>,
    ) -> Result<Vec<Message>> {
        if tx.is_none() && !self.stdout {
            panic!("enable stdout or provide tx channel");
        }

        let mut current_messages = messages;
        let max_iters = 12usize;

        for _ in 0..max_iters {
            // Split history / latest turn so OpenAI-compatible meshes get a real `messages` list
            // (sending only a space prompt caused mesh.x402 / fly mesh to return messages_required).
            let (history, prompt_msg) = match current_messages.split_last() {
                Some((last, rest)) => (rest.to_vec(), last.clone()),
                None => (
                    vec![],
                    Message::User {
                        content: OneOrMany::one(UserContent::text("hello")),
                    },
                ),
            };

            let response = self
                .agent
                .completion(prompt_msg, history)
                .await?
                .send()
                .await?;

            let mut saw_tool = false;
            let mut text_buf = String::new();

            for content in response.choice.iter() {
                match content {
                    AssistantContent::Text(text) => {
                        text_buf.push_str(&text.text);
                        if self.stdout {
                            print!("{}", text.text);
                            std::io::stdout().flush()?;
                        }
                        if let Some(tx) = &tx {
                            tx.send(LoopResponse::Message(text.text.clone())).await?;
                        }
                    }
                    AssistantContent::ToolCall(tool_call) => {
                        saw_tool = true;
                        let name = tool_call.function.name.clone();
                        let tool_id = tool_call.id.clone();
                        let params = tool_call.function.arguments.clone();

                        if !text_buf.is_empty() {
                            current_messages.push(Message::Assistant {
                                content: OneOrMany::one(AssistantContent::text(text_buf.clone())),
                            });
                            text_buf.clear();
                        }

                        current_messages.push(Message::Assistant {
                            content: OneOrMany::one(AssistantContent::tool_call(
                                tool_id.clone(),
                                name.clone(),
                                params.clone(),
                            )),
                        });

                        let result = self.agent.tools.call(&name, params.to_string()).await;

                        if self.stdout {
                            println!("\nTool result: {:?}", result);
                        }

                        let result_str = match &result {
                            Ok(content) => content.to_string(),
                            Err(err) => err.to_string(),
                        };

                        current_messages.push(Message::User {
                            content: OneOrMany::one(UserContent::tool_result(
                                tool_id,
                                OneOrMany::one(ToolResultContent::text(result_str.clone())),
                            )),
                        });

                        if let Some(tx) = &tx {
                            tx.send(LoopResponse::ToolCall {
                                name,
                                result: result_str,
                            })
                            .await?;
                        }
                    }
                }
            }

            if !text_buf.is_empty() {
                current_messages.push(Message::Assistant {
                    content: OneOrMany::one(AssistantContent::text(text_buf)),
                });
            }

            if !saw_tool {
                break;
            }
        }

        Ok(current_messages)
    }
}
