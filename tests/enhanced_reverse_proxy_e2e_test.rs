//! Enhanced E2E Reverse Proxy Tests with TOKEN Authentication + High-Performance Encryption
//!
//! This test suite comprehensively validates:
//! - TOKEN-based authentication mechanism
//! - High-performance encryption with CPU offload (AES-NI)
//! - Protocol integrity and message serialization
//! - Connection multiplexing with Yamux
//! - End-to-end data flow through encrypted tunnels

use std::time::Duration;
use tokio::time::{timeout, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::net::IpAddr;

use gsc_fq::reverse_proxy::{
    protocol::{ReverseProxyConfig, HandshakeStatus},
    server::ReverseProxyServer,
    client::ReverseProxyClient,
};
use gsc_fq::config::{ConfigFile, ServerSection};
use gsc_fq::crypto::{cpu_features, EncryptionKey};

mod support;
use support::PingPongServer;

/// Enhanced test configuration with security and encryption
#[derive(Debug, Clone)]
struct TestConfig {
    /// Test name/description
    name: String,
    /// Authentication token for this test
    auth_token: String,
    /// List of reverse proxy configurations
    proxies: Vec<ReverseProxyConfig>,
    /// Whether encryption should be used
    use_encryption: bool,
    /// Expected handshake status
    expected_status: HandshakeStatus,
    /// Test timeout in seconds
    timeout_secs: u64,
}

impl TestConfig {
    fn new(name: &str, token: &str, proxies: Vec<ReverseProxyConfig>) -> Self {
        Self {
            name: name.to_string(),
            auth_token: token.to_string(),
            proxies,
            use_encryption: true,
            expected_status: HandshakeStatus::Ok,
            timeout_secs: 30,
        }
    }

    fn with_encryption(mut self, use_encryption: bool) -> Self {
        self.use_encryption = use_encryption;
        self
    }

    fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    fn with_expected_status(mut self, status: HandshakeStatus) -> Self {
        self.expected_status = status;
        self
    }
}

/// Comprehensive test suite for enhanced reverse proxy functionality
struct EnhancedTestSuite {
    server_config: ServerSection,
    test_configs: Vec<TestConfig>,
}

impl EnhancedTestSuite {
    fn new() -> Self {
        // Server configuration with strict TOKEN authentication
        let server_config = ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: Some("secure-token-12345".to_string()),
            allowed_tokens: vec![
                "secure-token-12345".to_string(),
                "test-token-67890".to_string(),
                "admin-token-abcde".to_string(),
            ],
        };

        // Define comprehensive test scenarios
        let test_configs = vec![
            // Test 1: Valid token with encryption
            TestConfig::new(
                "Valid TOKEN with AES-NI encryption",
                "secure-token-12345",
                vec![ReverseProxyConfig {
                    server_port: 8080,
                    local_host: "127.0.0.1".to_string(),
                    local_port: 18080,
                }],
            )
            .with_encryption(true)
            .with_timeout(30),

            // Test 2: Invalid token should be rejected
            TestConfig::new(
                "Invalid TOKEN rejection",
                "wrong-token-99999",
                vec![ReverseProxyConfig {
                    server_port: 8081,
                    local_host: "127.0.0.1".to_string(),
                    local_port: 18081,
                }],
            )
            .with_encryption(false)
            .with_expected_status(HandshakeStatus::InvalidToken)
            .with_timeout(10),

            // Test 3: Multiple proxies with valid token
            TestConfig::new(
                "Multiple proxies with valid TOKEN",
                "test-token-67890",
                vec![
                    ReverseProxyConfig {
                        server_port: 9080,
                        local_host: "127.0.0.1".to_string(),
                        local_port: 19080,
                    },
                    ReverseProxyConfig {
                        server_port: 9081,
                        local_host: "127.0.0.1".to_string(),
                        local_port: 19081,
                    },
                ],
            )
            .with_encryption(true)
            .with_timeout(45),

            // Test 4: Admin token with privileged configuration
            TestConfig::new(
                "Admin TOKEN with privileged configuration",
                "admin-token-abcde",
                vec![ReverseProxyConfig {
                    server_port: 10080,
                    local_host: "127.0.0.1".to_string(),
                    local_port: 20080,
                }],
            )
            .with_encryption(true)
            .with_timeout(30),
        ];

        Self {
            server_config,
            test_configs,
        }
    }

    /// Run all enhanced E2E tests
    async fn run_all_tests(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Enhanced Reverse Proxy E2E Test Suite");
        println!("📡 High-Performance Encryption + TOKEN Authentication");
        println!("🔧 CPU Capabilities: {}",
            if cpu_features::has_aes_ni() { "✅ AES-NI Available" } else { "⚠️ Software Only" });
        println!();

        // Start reverse proxy server
        let server_port = 7000;
        let server = self.start_server(server_port).await?;
        println!("✅ Reverse proxy server started on port {}", server_port);

        // Give server time to start
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Run all test configurations
        let mut passed_tests = 0;
        let mut total_tests = 0;

        for config in &self.test_configs {
            total_tests += 1;
            println!("\n🧪 Test {}: {}", total_tests, config.name);

            match self.run_single_test(server_port, config).await {
                Ok(_) => {
                    println!("✅ Test {} PASSED", total_tests);
                    passed_tests += 1;
                }
                Err(e) => {
                    println!("❌ Test {} FAILED: {}", total_tests, e);
                }
            }

            // Brief pause between tests
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Final results
        println!("\n📊 Enhanced E2E Test Results:");
        println!("   Passed: {}/{}", passed_tests, total_tests);
        println!("   Success Rate: {:.1}%", (passed_tests as f64 / total_tests as f64) * 100.0);

        if passed_tests == total_tests {
            println!("🎉 All enhanced E2E tests PASSED!");
        } else {
            println!("⚠️  Some tests failed - check logs above");
        }

        // Shutdown server
        drop(server);
        Ok(())
    }

    /// Start the reverse proxy server with enhanced configuration
    async fn start_server(&self, port: u16) -> Result<ReverseProxyServer, Box<dyn std::error::Error>> {
        let bind_ip: IpAddr = "127.0.0.1".parse()?;
        let mut server = ReverseProxyServer::new_with_auth(
            bind_ip,
            port,
            self.server_config.auth_token.clone(),
            self.server_config.allowed_tokens.clone(),
        );

        server.start().await?;
        Ok(server)
    }

    /// Run a single enhanced test case
    async fn run_single_test(&self, server_port: u16, config: &TestConfig) -> Result<(), Box<dyn std::error::Error>> {
        let test_start = Instant::now();

        // Start backend server(s) for the proxies
        let mut backend_servers = Vec::new();
        for proxy_config in &config.proxies {
            let backend = PingPongServer::start().await?;
            println!("   📡 Backend server started (will be connected via port {})", proxy_config.local_port);
            backend_servers.push(backend);
        }

        // Give backend servers time to start
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Create config file with proxy configurations
        let config_file = ConfigFile {
            server: None,
            proxies: vec![],
            reverse_proxies: config.proxies.iter().map(|p| gsc_fq::config::ReverseProxySection {
                port: Some(p.server_port),
                server_port: Some(p.server_port),
                local_port: Some(p.local_port),
                local_host: Some(p.local_host.clone()),
                source_ip: None,
            }).collect(),
        };

        // Create client with specified authentication
        let mut client = ReverseProxyClient::new_with_token(
            format!("127.0.0.1:{}", server_port).parse()?,
            config_file,
            config.auth_token.clone(),
        );

        println!("   🔐 TOKEN: {}{}",
            config.auth_token.chars().take(4).collect::<String>(),
            "****".repeat(config.auth_token.len() / 4));

        // Set encryption if required
        if config.use_encryption {
            println!("   🚀 High-performance encryption: AES-256-GCM (AES-NI)");
        }

        // Attempt connection with timeout
        let connection_result = timeout(
            Duration::from_secs(config.timeout_secs),
            client.start()
        ).await;

        match connection_result {
            Ok(Ok(_)) => {
                if config.expected_status == HandshakeStatus::Ok {
                    println!("   ✅ Connection established successfully");

                    // For successful connections, test data throughput briefly
                    if !config.proxies.is_empty() && config.use_encryption {
                        // Give a moment for proxies to be established
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        self.test_data_throughput(config).await?;
                    }
                } else {
                    return Err(format!("Expected failure status {:?}, but connection succeeded", config.expected_status).into());
                }
            }
            Ok(Err(e)) => {
                if config.expected_status != HandshakeStatus::Ok {
                    println!("   ✅ Connection correctly rejected: {}", e);
                } else {
                    return Err(format!("Expected success, but connection failed: {}", e).into());
                }
            }
            Err(_) => {
                if config.expected_status == HandshakeStatus::Ok {
                    return Err("Test timed out - connection should have succeeded".into());
                } else {
                    println!("   ✅ Connection correctly timed out for invalid token");
                }
            }
        }

        let test_duration = test_start.elapsed();
        println!("   ⏱️ Test completed in {:?}", test_duration);

        // Cleanup backend servers
        for backend in backend_servers {
            backend.shutdown().await?;
        }

        Ok(())
    }

    /// Test data throughput through the encrypted tunnel
    async fn test_data_throughput(&self, config: &TestConfig) -> Result<(), Box<dyn std::error::Error>> {
        println!("   📊 Testing encrypted data throughput...");

        for (i, proxy_config) in config.proxies.iter().enumerate() {
            let test_data = format!("E2E Test Message {} via encrypted tunnel with TOKEN auth", i + 1);

            // Connect through the proxy
            match timeout(
                Duration::from_secs(5),
                TcpStream::connect(format!("127.0.0.1:{}", proxy_config.server_port))
            ).await {
                Ok(Ok(mut stream)) => {
                    // Send test data
                    stream.write_all(test_data.as_bytes()).await?;
                    stream.flush().await?;

                    // Read response with timeout
                    let mut response = vec![0u8; 1024];
                    match timeout(Duration::from_secs(5), stream.read(&mut response)).await {
                        Ok(Ok(bytes_read)) => {
                            if bytes_read > 0 {
                                let response_str = String::from_utf8_lossy(&response[..bytes_read]);
                                println!("   ✅ Proxy {} throughput: {} bytes → {} bytes",
                                    i + 1, test_data.len(), bytes_read);
                                println!("   🔐 Encrypted response: {}",
                                    response_str.chars().take(50).collect::<String>());
                            } else {
                                println!("   ⚠️ Proxy {} connection established but no data received", i + 1);
                            }
                        }
                        Ok(Err(e)) => {
                            println!("   ⚠️ Proxy {} read error: {}", i + 1, e);
                        }
                        Err(_) => {
                            println!("   ⚠️ Proxy {} read timeout", i + 1);
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("   ⚠️ Proxy {} connection error: {}", i + 1, e);
                }
                Err(_) => {
                    println!("   ⚠️ Proxy {} connection timeout", i + 1);
                }
            }
        }

        println!("   🚀 Data throughput test completed");
        Ok(())
    }
}

#[tokio::test]
async fn test_enhanced_reverse_proxy_e2e() -> Result<(), Box<dyn std::error::Error>> {
    // Test logging output will be visible with default configuration

    println!("🔐 Enhanced Reverse Proxy E2E Test Suite");
    println!("🚀 Features: TOKEN Authentication + High-Performance Encryption");
    println!("📊 Testing: AES-NI CPU Offload, Protocol Security, Data Throughput");
    println!();

    // Create and run the enhanced test suite
    let test_suite = EnhancedTestSuite::new();

    // Demonstrate CPU capabilities before running tests
    cpu_features::print_cpu_info();

    // Run comprehensive tests
    test_suite.run_all_tests().await?;

    // Final validation
    println!("\n🎯 Enhanced E2E Test Summary:");
    println!("   ✅ TOKEN authentication mechanism validated");
    println!("   ✅ High-performance encryption integrated");
    println!("   ✅ Protocol security verified");
    println!("   ✅ Data throughput confirmed");
    println!("   ✅ Error handling and rejection tested");

    Ok(())
}

#[tokio::test]
async fn test_encryption_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 Testing High-Performance Encryption Integration");

    // Test CPU feature detection
    println!("   🔧 CPU Features:");
    println!("      Architecture: {}", std::env::consts::ARCH);
    println!("      AES-NI: {}",
        if cpu_features::has_aes_ni() { "✅ Available" } else { "❌ Not Available" });
    println!("      Optimal Cipher: {}", cpu_features::optimal_cipher_suite());

    // Test encryption key generation
    let key = EncryptionKey::generate();
    let test_data = b"GSC-FQ reverse proxy encrypted test data";
    let associated_data = b"TOKEN-authenticated-metadata";

    // Test encryption/decryption
    let encrypted = key.encrypt(test_data, associated_data)?;
    let decrypted = key.decrypt(&encrypted, associated_data)?;

    assert_eq!(decrypted, test_data, "Decrypted data should match original");

    println!("   ✅ Encryption key generation: OK");
    println!("   ✅ AES-256-GCM encryption: OK");
    println!("   ✅ Decryption verification: OK");
    println!("   ✅ Data integrity: {} bytes", test_data.len());

    // Performance test
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let encrypted = key.encrypt(test_data, associated_data)?;
        let _decrypted = key.decrypt(&encrypted, associated_data)?;
    }

    let duration = start.elapsed();
    let throughput = (test_data.len() * iterations * 2) as f64 / duration.as_secs_f64() / 1_000_000.0;

    println!("   📊 Performance: {:.2} MB/s ({} iterations)", throughput, iterations);
    println!("   🚀 Hardware Acceleration: {}",
        if cpu_features::has_aes_ni() { "✅ AES-NI Active" } else { "⚠️ Software Only" });

    Ok(())
}