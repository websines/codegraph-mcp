use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, trace};

use super::protocol::{JsonRpcRequest, JsonRpcResponse};

pub trait Handler: Send + Sync {
    async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse;
}

/// Run the MCP server over stdio transport
///
/// Reads newline-delimited JSON from stdin, writes to stdout.
/// All logging/tracing goes to stderr only.
pub async fn run_stdio<H: Handler>(handler: H) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    debug!("MCP server started, listening on stdio");

    while let Some(line) = lines.next_line().await? {
        trace!("Received line: {}", line);

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                let error_response = JsonRpcResponse::error(
                    None,
                    super::protocol::PARSE_ERROR,
                    format!("Parse error: {}", e),
                );
                write_response(&mut stdout, &error_response).await?;
                continue;
            }
        };

        debug!(
            "Processing request: method={}, id={:?}",
            request.method, request.id
        );

        // Handle request
        let response = handler.handle(request).await;

        // Write response (unless it's a notification)
        if response.id.is_some() || response.error.is_some() {
            write_response(&mut stdout, &response).await?;
        }
    }

    debug!("MCP server shutting down");
    Ok(())
}

async fn write_response(
    stdout: &mut tokio::io::Stdout,
    response: &JsonRpcResponse,
) -> Result<()> {
    let json = serde_json::to_string(response)?;
    trace!("Sending response: {}", json);

    stdout.write_all(json.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::JsonRpcRequest;
    use serde_json::Value;

    struct TestHandler;

    impl Handler for TestHandler {
        async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
            JsonRpcResponse::success(
                request.id,
                serde_json::json!({"method": request.method}),
            )
        }
    }

    #[tokio::test]
    async fn test_handler_trait() {
        let handler = TestHandler;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(1)),
            method: "test".to_string(),
            params: None,
        };

        let response = handler.handle(request).await;
        assert!(response.result.is_some());
    }
}
