//! Streaming pipeline server for network-based text processing.
//!
//! This module provides a server that accepts connections and processes text
//! through rexpipe pipelines. Useful for long-running services that need to
//! apply consistent transformations to incoming data.
//!
//! # Protocol
//!
//! The server uses a simple line-based protocol:
//!
//! 1. Client sends pipeline configuration as JSON on a single line
//! 2. Client sends "---" delimiter
//! 3. Client sends text to process (line by line)
//! 4. Client sends "---" delimiter or EOF
//! 5. Server responds with processed text
//! 6. Server sends "---" delimiter
//!
//! # Example
//!
//! ```text
//! # Client sends:
//! {"step":[{"type":"substitute","pattern":"\\d+","replacement":"NUM"}]}
//! ---
//! There are 42 apples.
//! And 17 oranges.
//! ---
//!
//! # Server responds:
//! There are NUM apples.
//! And NUM oranges.
//! ---
//! ```

use crate::pipeline::PipelineConfig;
use crate::processor::StreamProcessor;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
#[cfg(feature = "async")]
use std::sync::Arc;

/// Configuration for the pipeline server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind to (e.g., "127.0.0.1:8080")
    pub bind_address: String,
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    /// Optional default pipeline configuration to use when none provided
    pub default_config: Option<PipelineConfig>,
    /// Timeout for reading from clients (in seconds)
    pub read_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".to_string(),
            max_connections: 10,
            default_config: None,
            read_timeout_secs: 30,
        }
    }
}

/// Streaming pipeline server.
///
/// The server listens for connections and processes text through rexpipe pipelines.
/// Each connection can optionally specify its own pipeline configuration, or use
/// the server's default configuration.
pub struct PipelineServer {
    config: ServerConfig,
}

impl PipelineServer {
    /// Create a new pipeline server with the given configuration.
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    /// Run the server, blocking until interrupted.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot bind to the specified address.
    pub fn run(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(&self.config.bind_address)?;
        log::info!("Pipeline server listening on {}", self.config.bind_address);

        let default_config = self.config.default_config.clone();
        let read_timeout = std::time::Duration::from_secs(self.config.read_timeout_secs);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let default_config = default_config.clone();
                    stream.set_read_timeout(Some(read_timeout)).ok();

                    thread::spawn(move || {
                        if let Err(e) = handle_connection(stream, default_config) {
                            log::error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Connection accept error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Run the server asynchronously using tokio.
    #[cfg(feature = "async")]
    pub async fn run_async(&self) -> std::io::Result<()> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
        use tokio::net::TcpListener as AsyncTcpListener;

        let listener = AsyncTcpListener::bind(&self.config.bind_address).await?;
        log::info!(
            "Pipeline server (async) listening on {}",
            self.config.bind_address
        );

        let default_config = Arc::new(self.config.default_config.clone());

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::debug!("Accepted connection from {}", addr);
                    let default_config = default_config.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection_async(stream, (*default_config).clone()).await {
                            log::error!("Connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Connection accept error: {}", e);
                }
            }
        }
    }
}

/// Protocol delimiter
const DELIMITER: &str = "---";

/// Handle a single client connection (synchronous).
fn handle_connection(
    stream: TcpStream,
    default_config: Option<PipelineConfig>,
) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    log::debug!("Handling connection from {}", peer_addr);

    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    let mut lines = reader.lines();

    // Read pipeline configuration (JSON) or use default
    let config = if let Some(Ok(first_line)) = lines.next() {
        if first_line.trim() == DELIMITER {
            // No config provided, use default
            match default_config {
                Some(cfg) => cfg,
                None => {
                    writeln!(writer, "ERROR: No pipeline configuration provided and no default configured")?;
                    writeln!(writer, "{}", DELIMITER)?;
                    writer.flush()?;
                    return Ok(());
                }
            }
        } else {
            // Parse JSON config
            match serde_json::from_str::<PipelineConfig>(&first_line) {
                Ok(cfg) => {
                    // Skip the delimiter after config
                    if let Some(Ok(delim)) = lines.next() {
                        if delim.trim() != DELIMITER {
                            writeln!(writer, "ERROR: Expected '{}' after configuration", DELIMITER)?;
                            writeln!(writer, "{}", DELIMITER)?;
                            writer.flush()?;
                            return Ok(());
                        }
                    }
                    cfg
                }
                Err(e) => {
                    writeln!(writer, "ERROR: Invalid pipeline configuration: {}", e)?;
                    writeln!(writer, "{}", DELIMITER)?;
                    writer.flush()?;
                    return Ok(());
                }
            }
        }
    } else {
        writeln!(writer, "ERROR: Empty request")?;
        writeln!(writer, "{}", DELIMITER)?;
        writer.flush()?;
        return Ok(());
    };

    // Create processor
    let mut processor = match StreamProcessor::new(config) {
        Ok(p) => p,
        Err(e) => {
            writeln!(writer, "ERROR: Failed to create processor: {}", e)?;
            writeln!(writer, "{}", DELIMITER)?;
            writer.flush()?;
            return Ok(());
        }
    };

    // Collect input until delimiter or EOF
    let mut input = String::new();
    for line in lines {
        match line {
            Ok(l) if l.trim() == DELIMITER => break,
            Ok(l) => {
                input.push_str(&l);
                input.push('\n');
            }
            Err(e) => {
                log::debug!("Read error (client may have disconnected): {}", e);
                break;
            }
        }
    }

    // Process input
    let mut output = Vec::new();
    match processor.process_stream(std::io::Cursor::new(input.as_bytes()), &mut output) {
        Ok(_) => {
            writer.write_all(&output)?;
        }
        Err(e) => {
            writeln!(writer, "ERROR: Processing failed: {}", e)?;
        }
    }

    writeln!(writer, "{}", DELIMITER)?;
    writer.flush()?;

    log::debug!("Completed request from {}", peer_addr);
    Ok(())
}

/// Handle a single client connection (asynchronous).
#[cfg(feature = "async")]
async fn handle_connection_async(
    stream: tokio::net::TcpStream,
    default_config: Option<PipelineConfig>,
) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};

    let peer_addr = stream.peer_addr()?;
    log::debug!("Handling async connection from {}", peer_addr);

    let (reader, mut writer) = stream.into_split();
    let mut lines = AsyncBufReader::new(reader).lines();

    // Read pipeline configuration (JSON) or use default
    let config = if let Some(first_line) = lines.next_line().await? {
        if first_line.trim() == DELIMITER {
            match default_config {
                Some(cfg) => cfg,
                None => {
                    writer.write_all(b"ERROR: No pipeline configuration provided and no default configured\n").await?;
                    writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
                    writer.flush().await?;
                    return Ok(());
                }
            }
        } else {
            match serde_json::from_str::<PipelineConfig>(&first_line) {
                Ok(cfg) => {
                    if let Some(delim) = lines.next_line().await? {
                        if delim.trim() != DELIMITER {
                            writer.write_all(format!("ERROR: Expected '{}' after configuration\n", DELIMITER).as_bytes()).await?;
                            writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
                            writer.flush().await?;
                            return Ok(());
                        }
                    }
                    cfg
                }
                Err(e) => {
                    writer.write_all(format!("ERROR: Invalid pipeline configuration: {}\n", e).as_bytes()).await?;
                    writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
                    writer.flush().await?;
                    return Ok(());
                }
            }
        }
    } else {
        writer.write_all(b"ERROR: Empty request\n").await?;
        writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
        writer.flush().await?;
        return Ok(());
    };

    // Create processor
    let mut processor = match StreamProcessor::new(config) {
        Ok(p) => p,
        Err(e) => {
            writer.write_all(format!("ERROR: Failed to create processor: {}\n", e).as_bytes()).await?;
            writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
            writer.flush().await?;
            return Ok(());
        }
    };

    // Collect input until delimiter or EOF
    let mut input = String::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim() == DELIMITER {
            break;
        }
        input.push_str(&line);
        input.push('\n');
    }

    // Process input
    let mut output = Vec::new();
    match processor.process_stream(std::io::Cursor::new(input.as_bytes()), &mut output) {
        Ok(_) => {
            writer.write_all(&output).await?;
        }
        Err(e) => {
            writer.write_all(format!("ERROR: Processing failed: {}\n", e).as_bytes()).await?;
        }
    }

    writer.write_all(format!("{}\n", DELIMITER).as_bytes()).await?;
    writer.flush().await?;

    log::debug!("Completed async request from {}", peer_addr);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_address, "127.0.0.1:8080");
        assert_eq!(config.max_connections, 10);
        assert!(config.default_config.is_none());
    }

    // Integration test that requires a running server
    // #[test]
    // fn test_server_connection() {
    //     // Start server in background thread
    //     let config = ServerConfig {
    //         bind_address: "127.0.0.1:0".to_string(), // Random port
    //         ..Default::default()
    //     };
    //     // Test would go here
    // }
}
