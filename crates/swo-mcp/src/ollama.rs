//! Built-in driver for a local language model served by Ollama.
//!
//! The MCP server works with any MCP client, but most people do not have one
//! wired to a local model. These functions let the same binary talk to Ollama
//! directly (`swo-mcp report`, `swo-mcp ask`), using the very same tool
//! functions and writing rules the MCP interface exposes.
//!
//! The only network traffic is HTTP to the Ollama endpoint, which defaults to
//! this machine. Nothing is sent anywhere else.

use crate::{prompts, reports, SwoServer};
use serde_json::{json, Value};
use std::time::Duration;

const DEFAULT_HOST: &str = "http://127.0.0.1:11434";
/// Model families tried first when no model is named; all handle tool calling.
const PREFERRED: &[&str] = &[
    "qwen3", "qwen2.5", "llama3.1", "llama3.2", "mistral", "gemma",
];
/// Tool-calling rounds allowed per question before giving up.
const MAX_TOOL_ROUNDS: usize = 6;

#[derive(Debug, Clone)]
pub struct Ollama {
    host: String,
    model: Option<String>,
    http: reqwest::Client,
}

impl Ollama {
    /// `model`: explicit choice, else `SWO_MODEL`, else picked from what is installed.
    /// Endpoint: `OLLAMA_HOST`, else this machine.
    pub fn new(model: Option<String>) -> Result<Self, String> {
        let mut host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_HOST.into());
        if !host.contains("://") {
            host = format!("http://{host}");
        }
        let http = reqwest::Client::builder()
            // Local models can take minutes on a laptop, especially on first load.
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            host: host.trim_end_matches('/').to_string(),
            model: model
                .or_else(|| std::env::var("SWO_MODEL").ok())
                .filter(|m| !m.is_empty()),
            http,
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    fn unreachable(&self, e: impl std::fmt::Display) -> String {
        format!(
            "cannot reach Ollama at {} ({e}). Is it running? Start it with `ollama serve`.",
            self.host
        )
    }

    /// The model to use, resolving and remembering a default on first call.
    pub async fn model(&mut self) -> Result<String, String> {
        if let Some(m) = &self.model {
            return Ok(m.clone());
        }
        let tags: Value = self
            .http
            .get(format!("{}/api/tags", self.host))
            .send()
            .await
            .map_err(|e| self.unreachable(e))?
            .json()
            .await
            .map_err(|e| format!("unexpected reply from Ollama: {e}"))?;
        let names: Vec<&str> = tags["models"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["name"].as_str()).collect())
            .unwrap_or_default();
        let pick = PREFERRED
            .iter()
            .find_map(|family| names.iter().find(|n| n.starts_with(family)))
            .or(names.first())
            .ok_or("Ollama has no models installed. Try `ollama pull qwen3:8b`.")?
            .to_string();
        self.model = Some(pick.clone());
        Ok(pick)
    }

    /// One non-streaming chat turn. Returns the assistant message object.
    async fn chat(&mut self, messages: &[Value], tools: Option<&[Value]>) -> Result<Value, String> {
        let model = self.model().await?;
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
            // Reasoning models otherwise spend minutes "thinking" about a task
            // that is rewording supplied facts.
            "think": false,
            "options": { "temperature": 0.2, "num_ctx": 16384 },
        });
        if let Some(tools) = tools {
            body["tools"] = json!(tools);
        }
        match self.post_chat(&body).await? {
            // Models without a thinking mode reject the `think` flag; retry without it.
            Err(e) if e.contains("think") => {
                if let Some(o) = body.as_object_mut() {
                    o.remove("think");
                }
                self.post_chat(&body).await?
            }
            reply => reply,
        }
    }

    /// Outer error: transport. Inner error: Ollama refused the request.
    async fn post_chat(&self, body: &Value) -> Result<Result<Value, String>, String> {
        let resp = self
            .http
            .post(format!("{}/api/chat", self.host))
            .json(body)
            .send()
            .await
            .map_err(|e| self.unreachable(e))?;
        let ok = resp.status().is_success();
        let value: Value = resp
            .json()
            .await
            .map_err(|e| format!("unexpected reply from Ollama: {e}"))?;
        Ok(if ok {
            Ok(value["message"].clone())
        } else {
            Err(format!(
                "Ollama error: {}",
                value["error"].as_str().unwrap_or("unknown")
            ))
        })
    }
}

/// Drop any `<think>...</think>` block a reasoning model leaves in its answer.
pub fn strip_thinking(text: &str) -> String {
    let mut out = text.to_string();
    while let (Some(start), Some(end)) = (out.find("<think>"), out.find("</think>")) {
        if end < start {
            break;
        }
        out.replace_range(start..end + "</think>".len(), "");
    }
    out.trim().to_string()
}

fn strip_keys(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            map.retain(|k, _| !keys.contains(&k.as_str()));
            map.values_mut().for_each(|v| strip_keys(v, keys));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| strip_keys(v, keys)),
        _ => {}
    }
}

/// Have the local model write a report from the current dashboard, and save it.
pub async fn write_report(
    server: &SwoServer,
    llm: &mut Ollama,
) -> Result<(reports::StoredReport, std::path::PathBuf), String> {
    let dashboard = server.dashboard()?;
    // Provenance is added to the saved report by `reports::compose`; leaving the
    // URLs out of the prompt keeps a small model's attention on the readings.
    let mut value = serde_json::to_value(&dashboard).map_err(|e| e.to_string())?;
    strip_keys(&mut value, &["source", "sources", "data_fingerprint"]);
    let data = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let messages = [
        json!({ "role": "system", "content": prompts::REPORT_RULES }),
        json!({ "role": "user", "content": format!(
            "Current time: {}. Here is the dashboard data as JSON. All times are UTC.\n\n{data}\n\nWrite the report now.{}",
            dashboard.now,
            prompts::stale_addendum(dashboard.stale_data_warning.as_deref())
        )}),
    ];
    let reply = llm.chat(&messages, None).await?;
    let body = strip_thinking(reply["content"].as_str().unwrap_or_default());
    let model = llm.model().await?;
    server.store_report(&body, &model)
}

/// Write a new report only when the saved one is out of date. Returns the
/// reasons it was rewritten, or `None` when it was already current.
pub async fn refresh_report(
    server: &SwoServer,
    llm: &mut Ollama,
    force: bool,
) -> Result<Option<Vec<String>>, String> {
    let saved = server.report_store()?.latest()?;
    let fresh = reports::freshness(saved.as_ref(), &server.dashboard()?, server.now());
    if fresh.is_current && !force {
        return Ok(None);
    }
    write_report(server, llm).await?;
    Ok(Some(if fresh.reasons.is_empty() {
        vec!["forced".into()]
    } else {
        fresh.reasons
    }))
}

/// Answer a question, letting the model call the read-only tools it needs.
pub async fn ask(server: &SwoServer, llm: &mut Ollama, question: &str) -> Result<String, String> {
    let tools: Vec<Value> = server
        .chat_tools()
        .into_iter()
        .map(|t| {
            json!({ "type": "function", "function": {
                "name": t.name,
                "description": t.description.unwrap_or_default(),
                "parameters": Value::Object((*t.input_schema).clone()),
            }})
        })
        .collect();
    let system = format!(
        "You answer questions about the Space Weather Observatory dashboard using the tools provided. \
         Always call a tool before answering: explain_reading for what a reading means, get_dashboard for \
         current values. {}",
        prompts::EXPLAIN_RULES
    );
    // Stated up front so the answer carries it even if the model never opens the dashboard.
    let system = match server.dashboard()?.stale_data_warning {
        Some(warning) => format!(
            "{system} IMPORTANT: {warning} Tell the reader this whenever you quote a value."
        ),
        None => system,
    };
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": question }),
    ];
    for _ in 0..MAX_TOOL_ROUNDS {
        let reply = llm.chat(&messages, Some(&tools)).await?;
        let calls = reply["tool_calls"].as_array().cloned().unwrap_or_default();
        messages.push(reply.clone());
        if calls.is_empty() {
            return Ok(strip_thinking(
                reply["content"].as_str().unwrap_or_default(),
            ));
        }
        for call in calls {
            let name = call["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            eprintln!(
                "swo-mcp: model called {name}({})",
                call["function"]["arguments"]
            );
            // A failed call goes back to the model as text so it can correct itself.
            let result = match server
                .call_tool_json(&name, call["function"]["arguments"].clone())
                .await
            {
                Ok(v) => v.to_string(),
                Err(e) => format!("error: {e}"),
            };
            messages.push(json!({ "role": "tool", "tool_name": name, "content": result }));
        }
    }
    Err(format!(
        "the model kept calling tools for {MAX_TOOL_ROUNDS} rounds without answering"
    ))
}
