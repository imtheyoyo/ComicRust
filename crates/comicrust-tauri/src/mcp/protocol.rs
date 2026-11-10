//! Protocole MCP (JSON-RPC 2.0) : initialisation, liste et appel des outils.
//!
//! Même protocole que le crate `egui-mcp` de la version egui ; ici les outils sont
//! traités par une fonction fournie par [`super`] (pilotage de la WebView).

use std::sync::Arc;

use serde_json::{json, Value};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Réglages du serveur.
#[derive(Clone, Debug)]
pub struct Options {
    pub name: String,
    pub version: String,
    pub port: u16,
    pub token: Option<String>,
    pub instructions: String,
}

/// Outil déclaré au client.
#[derive(Clone, Debug)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Résultat d'un outil.
#[derive(Debug, Clone)]
pub enum Reply {
    Text(String),
    Json(Value),
    /// Image PNG encodée en base64, avec un texte d'accompagnement.
    Image {
        png_base64: String,
        text: String,
    },
    Error(String),
}

/// Traitement d'un appel d'outil (sur le fil de la connexion HTTP).
pub type Handler = Arc<dyn Fn(&str, &Value) -> Reply + Send + Sync>;

pub struct Server {
    options: Options,
    tools: Vec<Tool>,
    handler: Handler,
}

impl Server {
    pub fn new(options: Options, tools: Vec<Tool>, handler: Handler) -> Self {
        Server { options, tools, handler }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Traite un message (ou un lot). `None` : rien à renvoyer (notifications).
    pub fn handle(&self, msg: Value) -> Option<Value> {
        match msg {
            Value::Array(batch) => {
                let out: Vec<Value> = batch.into_iter().filter_map(|m| self.handle_one(m)).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            m => self.handle_one(m),
        }
    }

    fn handle_one(&self, msg: Value) -> Option<Value> {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        // Notification (pas d'id) ou réponse du client : rien à renvoyer.
        let id = msg.get("id").cloned()?;
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })).collect::<Vec<_>>()
            })),
            "tools/call" => Ok(self.call(&params)),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err(format!("méthode inconnue : {method}")),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(message) => json!({
                "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": message }
            }),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked.filter(|v| PROTOCOL_VERSIONS.contains(v)).unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": self.options.name, "version": self.options.version },
            "instructions": self.options.instructions,
        })
    }

    fn call(&self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let reply = if self.tools.iter().any(|t| t.name == name) {
            (self.handler)(name, &args)
        } else {
            Reply::Error(format!("outil inconnu : {name}"))
        };
        tool_result(reply)
    }
}

fn tool_result(reply: Reply) -> Value {
    match reply {
        Reply::Text(t) => json!({ "content": [{ "type": "text", "text": t }], "isError": false }),
        Reply::Json(v) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
            "structuredContent": if v.is_object() { v.clone() } else { json!({ "value": v }) },
            "isError": false
        }),
        Reply::Image { png_base64, text } => json!({
            "content": [
                { "type": "image", "data": png_base64, "mimeType": "image/png" },
                { "type": "text", "text": text }
            ],
            "isError": false
        }),
        Reply::Error(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serveur() -> Server {
        let options =
            Options { name: "test".into(), version: "1".into(), port: 0, token: None, instructions: String::new() };
        let tools = vec![Tool { name: "echo", description: "écho", input_schema: json!({"type": "object"}) }];
        Server::new(options, tools, Arc::new(|_, args| Reply::Json(args.clone())))
    }

    #[test]
    fn echanges() {
        let s = serveur();
        let init =
            s.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}));
        assert_eq!(init.unwrap()["result"]["protocolVersion"], "2024-11-05");
        assert!(s.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let r = s
            .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"a":1}}}));
        assert_eq!(r.unwrap()["result"]["structuredContent"]["a"], 1);
        let r = s.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"absent"}}));
        assert_eq!(r.unwrap()["result"]["isError"], true);
    }
}
