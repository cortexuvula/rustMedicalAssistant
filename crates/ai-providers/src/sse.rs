//! Server-Sent Events stream parser for AI provider streaming responses.

use std::pin::Pin;
use eventsource_stream::Eventsource;
use futures_core::Stream;
use reqwest::Response;
use tokio_stream::StreamExt;

/// Parse a streaming HTTP response as SSE, yielding non-empty, non-`[DONE]` data lines.
pub fn parse_sse_response(
    response: Response,
) -> Pin<Box<dyn Stream<Item = Result<String, String>> + Send>> {
    let stream = response
        .bytes_stream()
        .eventsource()
        .filter_map(|event_result| {
            match event_result {
                Err(e) => Some(Err(e.to_string())),
                Ok(event) => {
                    let data = event.data;
                    if data.is_empty() || data == "[DONE]" {
                        None
                    } else {
                        Some(Ok(data))
                    }
                }
            }
        });

    Box::pin(stream)
}
