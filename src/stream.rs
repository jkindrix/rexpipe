//! Streaming source and sink abstractions for continuous data processing.
//!
//! This module provides URI-based configuration for input sources and output sinks,
//! enabling flexible streaming pipelines.
//!
//! # Supported Sources
//!
//! - `stdin://` - Read from standard input
//! - `file:///path/to/file` - Read from a file
//! - `tcp://host:port` - Accept connections on TCP socket
//! - `udp://host:port` - Receive UDP datagrams
//!
//! # Supported Sinks
//!
//! - `stdout://` - Write to standard output
//! - `stderr://` - Write to standard error
//! - `file:///path/to/file` - Write to a file
//! - `tcp://host:port` - Send to TCP socket
//! - `udp://host:port` - Send UDP datagrams
//!
//! # Example
//!
//! ```text
//! rexpipe --config pipeline.toml --input tcp://0.0.0.0:5140 --output file:///var/log/processed.log
//! ```

use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::fs::File;

/// Parsed URI for a stream source or sink.
#[derive(Debug, Clone)]
pub struct StreamUri {
    /// The scheme (stdin, stdout, file, tcp, udp, kafka, etc.)
    pub scheme: String,
    /// The host (for network protocols)
    pub host: Option<String>,
    /// The port (for network protocols)
    pub port: Option<u16>,
    /// The path (for file:// URIs)
    pub path: Option<PathBuf>,
    /// The topic (for kafka:// URIs)
    pub topic: Option<String>,
    /// Consumer group ID (for Kafka consumers)
    pub group_id: Option<String>,
}

impl StreamUri {
    /// Parse a URI string into a StreamUri.
    ///
    /// Supported formats:
    /// - `stdin://`
    /// - `stdout://`
    /// - `stderr://`
    /// - `file:///path/to/file`
    /// - `tcp://host:port`
    /// - `udp://host:port`
    /// - `kafka://host:port/topic` or `kafka://host:port/topic?group_id=my-group`
    pub fn parse(uri: &str) -> Result<Self, String> {
        // Handle simple schemes without authority
        if uri == "stdin://" || uri == "stdin:" {
            return Ok(StreamUri {
                scheme: "stdin".to_string(),
                host: None,
                port: None,
                path: None,
                topic: None,
                group_id: None,
            });
        }
        if uri == "stdout://" || uri == "stdout:" {
            return Ok(StreamUri {
                scheme: "stdout".to_string(),
                host: None,
                port: None,
                path: None,
                topic: None,
                group_id: None,
            });
        }
        if uri == "stderr://" || uri == "stderr:" {
            return Ok(StreamUri {
                scheme: "stderr".to_string(),
                host: None,
                port: None,
                path: None,
                topic: None,
                group_id: None,
            });
        }

        // Parse scheme://...
        let parts: Vec<&str> = uri.splitn(2, "://").collect();
        if parts.len() != 2 {
            return Err(format!("Invalid URI format: {}", uri));
        }

        let scheme = parts[0].to_lowercase();
        let rest = parts[1];

        match scheme.as_str() {
            "file" => {
                // file:///path/to/file or file://path/to/file
                let path = if rest.starts_with('/') {
                    rest.to_string()
                } else {
                    format!("/{}", rest)
                };
                Ok(StreamUri {
                    scheme,
                    host: None,
                    port: None,
                    path: Some(PathBuf::from(path)),
                    topic: None,
                    group_id: None,
                })
            }
            "tcp" | "udp" => {
                // tcp://host:port or udp://host:port
                let parts: Vec<&str> = rest.rsplitn(2, ':').collect();
                if parts.len() != 2 {
                    return Err(format!("Invalid {} URI - expected host:port: {}", scheme, uri));
                }
                let port: u16 = parts[0]
                    .parse()
                    .map_err(|_| format!("Invalid port number: {}", parts[0]))?;
                let host = parts[1].to_string();
                Ok(StreamUri {
                    scheme,
                    host: Some(host),
                    port: Some(port),
                    path: None,
                    topic: None,
                    group_id: None,
                })
            }
            "kafka" => {
                // kafka://host:port/topic or kafka://host:port/topic?group_id=my-group
                // Split off query string first
                let (path_part, query) = if let Some(idx) = rest.find('?') {
                    (&rest[..idx], Some(&rest[idx + 1..]))
                } else {
                    (rest, None)
                };

                // Parse group_id from query string
                let group_id = query.and_then(|q| {
                    q.split('&')
                        .find_map(|param| {
                            let kv: Vec<&str> = param.splitn(2, '=').collect();
                            if kv.len() == 2 && kv[0] == "group_id" {
                                Some(kv[1].to_string())
                            } else {
                                None
                            }
                        })
                });

                // Split host:port/topic
                let parts: Vec<&str> = path_part.splitn(2, '/').collect();
                if parts.is_empty() {
                    return Err(format!("Invalid kafka URI - expected host:port/topic: {}", uri));
                }

                let host_port = parts[0];
                let topic = if parts.len() > 1 && !parts[1].is_empty() {
                    Some(parts[1].to_string())
                } else {
                    None
                };

                // Parse host:port
                let hp_parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
                if hp_parts.len() != 2 {
                    return Err(format!("Invalid kafka URI - expected host:port: {}", uri));
                }
                let port: u16 = hp_parts[0]
                    .parse()
                    .map_err(|_| format!("Invalid port number: {}", hp_parts[0]))?;
                let host = hp_parts[1].to_string();

                Ok(StreamUri {
                    scheme,
                    host: Some(host),
                    port: Some(port),
                    path: None,
                    topic,
                    group_id,
                })
            }
            _ => Err(format!("Unsupported URI scheme: {}", scheme)),
        }
    }

    /// Get the address string for network protocols (host:port).
    pub fn address(&self) -> Option<String> {
        match (&self.host, self.port) {
            (Some(host), Some(port)) => Some(format!("{}:{}", host, port)),
            _ => None,
        }
    }
}

/// Trait for stream input sources.
pub trait StreamSource: Send {
    /// Read the next line from the source.
    /// Returns None when the source is exhausted.
    fn read_line(&mut self) -> io::Result<Option<String>>;

    /// Read all remaining data from the source.
    fn read_to_string(&mut self) -> io::Result<String> {
        let mut result = String::new();
        while let Some(line) = self.read_line()? {
            result.push_str(&line);
            result.push('\n');
        }
        Ok(result)
    }
}

/// Trait for stream output sinks.
pub trait StreamSink: Send {
    /// Write a line to the sink.
    fn write_line(&mut self, line: &str) -> io::Result<()>;

    /// Write raw bytes to the sink.
    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()>;

    /// Flush the sink.
    fn flush(&mut self) -> io::Result<()>;
}

/// Standard input source.
pub struct StdinSource {
    reader: BufReader<io::Stdin>,
}

impl StdinSource {
    pub fn new() -> Self {
        StdinSource {
            reader: BufReader::new(io::stdin()),
        }
    }
}

impl Default for StdinSource {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamSource for StdinSource {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => {
                // Remove trailing newline
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => Err(e),
        }
    }
}

/// File input source.
pub struct FileSource {
    reader: BufReader<File>,
}

impl FileSource {
    pub fn open(path: &std::path::Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(FileSource {
            reader: BufReader::new(file),
        })
    }
}

impl StreamSource for FileSource {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => Err(e),
        }
    }
}

/// TCP listener source - accepts connections and reads from them.
pub struct TcpSource {
    listener: TcpListener,
    current_stream: Option<BufReader<TcpStream>>,
}

impl TcpSource {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(TcpSource {
            listener,
            current_stream: None,
        })
    }

    fn accept_next(&mut self) -> io::Result<()> {
        let (stream, _addr) = self.listener.accept()?;
        self.current_stream = Some(BufReader::new(stream));
        Ok(())
    }
}

impl StreamSource for TcpSource {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        loop {
            // If we have a current stream, try to read from it
            if let Some(ref mut reader) = self.current_stream {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // Connection closed, accept next
                        self.current_stream = None;
                        self.accept_next()?;
                        continue;
                    }
                    Ok(_) => {
                        if line.ends_with('\n') {
                            line.pop();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                        }
                        return Ok(Some(line));
                    }
                    Err(e) => return Err(e),
                }
            } else {
                // No current stream, accept a connection
                self.accept_next()?;
            }
        }
    }
}

/// UDP socket source.
pub struct UdpSource {
    socket: UdpSocket,
    buffer: Vec<u8>,
}

impl UdpSource {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        Ok(UdpSource {
            socket,
            buffer: vec![0u8; 65535],
        })
    }
}

impl StreamSource for UdpSource {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        let (len, _addr) = self.socket.recv_from(&mut self.buffer)?;
        let data = String::from_utf8_lossy(&self.buffer[..len]).to_string();
        // Trim trailing newline if present
        let trimmed = data.trim_end_matches(|c| c == '\n' || c == '\r');
        Ok(Some(trimmed.to_string()))
    }
}

/// Standard output sink.
pub struct StdoutSink {
    writer: BufWriter<io::Stdout>,
}

impl StdoutSink {
    pub fn new() -> Self {
        StdoutSink {
            writer: BufWriter::new(io::stdout()),
        }
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamSink for StdoutSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.writer, "{}", line)
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// Standard error sink.
pub struct StderrSink {
    writer: BufWriter<io::Stderr>,
}

impl StderrSink {
    pub fn new() -> Self {
        StderrSink {
            writer: BufWriter::new(io::stderr()),
        }
    }
}

impl Default for StderrSink {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamSink for StderrSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.writer, "{}", line)
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// File output sink.
pub struct FileSink {
    writer: BufWriter<File>,
}

impl FileSink {
    pub fn create(path: &std::path::Path) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(FileSink {
            writer: BufWriter::new(file),
        })
    }

    pub fn append(path: &std::path::Path) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(FileSink {
            writer: BufWriter::new(file),
        })
    }
}

impl StreamSink for FileSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.writer, "{}", line)
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// TCP connection sink.
pub struct TcpSink {
    writer: BufWriter<TcpStream>,
}

impl TcpSink {
    pub fn connect(addr: &str) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(TcpSink {
            writer: BufWriter::new(stream),
        })
    }
}

impl StreamSink for TcpSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.writer, "{}", line)
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// UDP socket sink.
pub struct UdpSink {
    socket: UdpSocket,
    target_addr: String,
}

impl UdpSink {
    pub fn connect(addr: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(UdpSink {
            socket,
            target_addr: addr.to_string(),
        })
    }
}

impl StreamSink for UdpSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let data = format!("{}\n", line);
        self.socket.send_to(data.as_bytes(), &self.target_addr)?;
        Ok(())
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.socket.send_to(data, &self.target_addr)?;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // UDP is connectionless, no buffering
    }
}

// ============================================================================
// Kafka Source and Sink (requires `kafka` feature)
// ============================================================================

/// Kafka consumer source - reads messages from a Kafka topic.
#[cfg(feature = "kafka")]
pub struct KafkaSource {
    consumer: rdkafka::consumer::StreamConsumer,
    runtime: tokio::runtime::Runtime,
}

#[cfg(feature = "kafka")]
impl KafkaSource {
    /// Create a new Kafka consumer for the given broker and topic.
    ///
    /// # Arguments
    /// * `brokers` - Kafka broker address (e.g., "localhost:9092")
    /// * `topic` - Topic to consume from
    /// * `group_id` - Consumer group ID (defaults to "rexpipe-consumer" if None)
    pub fn new(brokers: &str, topic: &str, group_id: Option<&str>) -> io::Result<Self> {
        use rdkafka::consumer::{Consumer, StreamConsumer};
        use rdkafka::ClientConfig;

        let group = group_id.unwrap_or("rexpipe-consumer");

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .create()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create Kafka consumer: {}", e)))?;

        consumer
            .subscribe(&[topic])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to subscribe to topic: {}", e)))?;

        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create Tokio runtime: {}", e)))?;

        Ok(KafkaSource { consumer, runtime })
    }
}

#[cfg(feature = "kafka")]
impl StreamSource for KafkaSource {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        use rdkafka::Message;

        let message = self.runtime.block_on(async {
            use futures::StreamExt;
            self.consumer.stream().next().await
        });

        match message {
            Some(Ok(msg)) => {
                match msg.payload_view::<str>() {
                    Some(Ok(text)) => {
                        let trimmed = text.trim_end_matches(|c| c == '\n' || c == '\r');
                        Ok(Some(trimmed.to_string()))
                    }
                    Some(Err(_)) => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 in Kafka message")),
                    None => Ok(Some(String::new())), // Empty message
                }
            }
            Some(Err(e)) => Err(io::Error::new(io::ErrorKind::Other, format!("Kafka error: {}", e))),
            None => Ok(None), // Stream ended
        }
    }
}

/// Kafka producer sink - writes messages to a Kafka topic.
#[cfg(feature = "kafka")]
pub struct KafkaSink {
    producer: rdkafka::producer::FutureProducer,
    topic: String,
    runtime: tokio::runtime::Runtime,
}

#[cfg(feature = "kafka")]
impl KafkaSink {
    /// Create a new Kafka producer for the given broker and topic.
    ///
    /// # Arguments
    /// * `brokers` - Kafka broker address (e.g., "localhost:9092")
    /// * `topic` - Topic to produce to
    pub fn new(brokers: &str, topic: &str) -> io::Result<Self> {
        use rdkafka::producer::FutureProducer;
        use rdkafka::ClientConfig;

        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create Kafka producer: {}", e)))?;

        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create Tokio runtime: {}", e)))?;

        Ok(KafkaSink {
            producer,
            topic: topic.to_string(),
            runtime,
        })
    }
}

#[cfg(feature = "kafka")]
impl StreamSink for KafkaSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        use rdkafka::producer::FutureRecord;
        use std::time::Duration;

        let record = FutureRecord::to(&self.topic)
            .payload(line)
            .key("");

        let result = self.runtime.block_on(async {
            self.producer.send(record, Duration::from_secs(5)).await
        });

        match result {
            Ok(_) => Ok(()),
            Err((e, _)) => Err(io::Error::new(io::ErrorKind::Other, format!("Failed to send to Kafka: {}", e))),
        }
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        use rdkafka::producer::FutureRecord;
        use std::time::Duration;

        let record = FutureRecord::to(&self.topic)
            .payload(data)
            .key("");

        let result = self.runtime.block_on(async {
            self.producer.send(record, Duration::from_secs(5)).await
        });

        match result {
            Ok(_) => Ok(()),
            Err((e, _)) => Err(io::Error::new(io::ErrorKind::Other, format!("Failed to send to Kafka: {}", e))),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        use std::time::Duration;

        self.runtime.block_on(async {
            self.producer.flush(Duration::from_secs(5))
        }).map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to flush Kafka producer: {}", e)))
    }
}

/// Create a source from a URI.
pub fn create_source(uri: &StreamUri) -> io::Result<Box<dyn StreamSource>> {
    match uri.scheme.as_str() {
        "stdin" => Ok(Box::new(StdinSource::new())),
        "file" => {
            let path = uri.path.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "File path required")
            })?;
            Ok(Box::new(FileSource::open(path)?))
        }
        "tcp" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "TCP address required")
            })?;
            Ok(Box::new(TcpSource::bind(&addr)?))
        }
        "udp" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "UDP address required")
            })?;
            Ok(Box::new(UdpSource::bind(&addr)?))
        }
        #[cfg(feature = "kafka")]
        "kafka" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Kafka broker address required")
            })?;
            let topic = uri.topic.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Kafka topic required")
            })?;
            Ok(Box::new(KafkaSource::new(&addr, topic, uri.group_id.as_deref())?))
        }
        #[cfg(not(feature = "kafka"))]
        "kafka" => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Kafka support not enabled. Build with --features kafka",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unsupported source scheme: {}", uri.scheme),
        )),
    }
}

/// Create a sink from a URI.
pub fn create_sink(uri: &StreamUri) -> io::Result<Box<dyn StreamSink>> {
    match uri.scheme.as_str() {
        "stdout" => Ok(Box::new(StdoutSink::new())),
        "stderr" => Ok(Box::new(StderrSink::new())),
        "file" => {
            let path = uri.path.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "File path required")
            })?;
            Ok(Box::new(FileSink::create(path)?))
        }
        "tcp" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "TCP address required")
            })?;
            Ok(Box::new(TcpSink::connect(&addr)?))
        }
        "udp" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "UDP address required")
            })?;
            Ok(Box::new(UdpSink::connect(&addr)?))
        }
        #[cfg(feature = "kafka")]
        "kafka" => {
            let addr = uri.address().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Kafka broker address required")
            })?;
            let topic = uri.topic.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Kafka topic required")
            })?;
            Ok(Box::new(KafkaSink::new(&addr, topic)?))
        }
        #[cfg(not(feature = "kafka"))]
        "kafka" => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Kafka support not enabled. Build with --features kafka",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unsupported sink scheme: {}", uri.scheme),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stdin_uri() {
        let uri = StreamUri::parse("stdin://").unwrap();
        assert_eq!(uri.scheme, "stdin");
        assert!(uri.host.is_none());
        assert!(uri.port.is_none());
    }

    #[test]
    fn test_parse_stdout_uri() {
        let uri = StreamUri::parse("stdout://").unwrap();
        assert_eq!(uri.scheme, "stdout");
    }

    #[test]
    fn test_parse_file_uri() {
        let uri = StreamUri::parse("file:///var/log/test.log").unwrap();
        assert_eq!(uri.scheme, "file");
        assert_eq!(uri.path, Some(PathBuf::from("/var/log/test.log")));
    }

    #[test]
    fn test_parse_tcp_uri() {
        let uri = StreamUri::parse("tcp://127.0.0.1:5140").unwrap();
        assert_eq!(uri.scheme, "tcp");
        assert_eq!(uri.host, Some("127.0.0.1".to_string()));
        assert_eq!(uri.port, Some(5140));
    }

    #[test]
    fn test_parse_udp_uri() {
        let uri = StreamUri::parse("udp://0.0.0.0:514").unwrap();
        assert_eq!(uri.scheme, "udp");
        assert_eq!(uri.host, Some("0.0.0.0".to_string()));
        assert_eq!(uri.port, Some(514));
    }

    #[test]
    fn test_invalid_uri() {
        assert!(StreamUri::parse("invalid").is_err());
        assert!(StreamUri::parse("unknown://localhost:80").is_err());
    }

    #[test]
    fn test_parse_kafka_uri() {
        let uri = StreamUri::parse("kafka://localhost:9092/my-topic").unwrap();
        assert_eq!(uri.scheme, "kafka");
        assert_eq!(uri.host, Some("localhost".to_string()));
        assert_eq!(uri.port, Some(9092));
        assert_eq!(uri.topic, Some("my-topic".to_string()));
        assert!(uri.group_id.is_none());
    }

    #[test]
    fn test_parse_kafka_uri_with_group() {
        let uri = StreamUri::parse("kafka://broker.example.com:9092/logs?group_id=my-consumer-group").unwrap();
        assert_eq!(uri.scheme, "kafka");
        assert_eq!(uri.host, Some("broker.example.com".to_string()));
        assert_eq!(uri.port, Some(9092));
        assert_eq!(uri.topic, Some("logs".to_string()));
        assert_eq!(uri.group_id, Some("my-consumer-group".to_string()));
    }

    #[test]
    fn test_parse_kafka_uri_without_topic() {
        let uri = StreamUri::parse("kafka://localhost:9092/").unwrap();
        assert_eq!(uri.scheme, "kafka");
        assert_eq!(uri.host, Some("localhost".to_string()));
        assert_eq!(uri.port, Some(9092));
        assert!(uri.topic.is_none());
    }
}
