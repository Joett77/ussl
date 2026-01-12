//! TLS configuration and utilities for USSL
//!
//! Provides TLS support using rustls for secure connections.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// TLS configuration for USSL servers
#[derive(Clone)]
pub struct TlsConfig {
    acceptor: TlsAcceptor,
}

impl TlsConfig {
    /// Create TLS config from certificate and key files
    ///
    /// # Arguments
    /// * `cert_path` - Path to PEM-encoded certificate file (can contain chain)
    /// * `key_path` - Path to PEM-encoded private key file
    ///
    /// # Example
    /// ```ignore
    /// let tls = TlsConfig::from_pem("cert.pem", "key.pem")?;
    /// ```
    pub fn from_pem<P: AsRef<Path>>(
        cert_path: P,
        key_path: P,
    ) -> Result<Self, TlsError> {
        let certs = load_certs(cert_path.as_ref())?;
        let key = load_private_key(key_path.as_ref())?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TlsError::Config(e.to_string()))?;

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }

    /// Get the TLS acceptor for accepting connections
    pub fn acceptor(&self) -> &TlsAcceptor {
        &self.acceptor
    }
}

/// Load certificates from a PEM file
fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let file = File::open(path)
        .map_err(|e| TlsError::CertLoad(format!("{}: {}", path.display(), e)))?;
    let mut reader = BufReader::new(file);

    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::CertLoad(format!("Failed to parse certificates: {}", e)))?;

    if certs.is_empty() {
        return Err(TlsError::CertLoad("No certificates found in file".into()));
    }

    Ok(certs)
}

/// Load private key from a PEM file
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    let file = File::open(path)
        .map_err(|e| TlsError::KeyLoad(format!("{}: {}", path.display(), e)))?;
    let mut reader = BufReader::new(file);

    // Try to read PKCS#8 keys first, then RSA keys, then EC keys
    loop {
        match rustls_pemfile::read_one(&mut reader) {
            Ok(Some(rustls_pemfile::Item::Pkcs1Key(key))) => {
                return Ok(PrivateKeyDer::Pkcs1(key));
            }
            Ok(Some(rustls_pemfile::Item::Pkcs8Key(key))) => {
                return Ok(PrivateKeyDer::Pkcs8(key));
            }
            Ok(Some(rustls_pemfile::Item::Sec1Key(key))) => {
                return Ok(PrivateKeyDer::Sec1(key));
            }
            Ok(Some(_)) => {
                // Skip other items (certificates, etc.)
                continue;
            }
            Ok(None) => {
                return Err(TlsError::KeyLoad("No private key found in file".into()));
            }
            Err(e) => {
                return Err(TlsError::KeyLoad(format!("Failed to parse key: {}", e)));
            }
        }
    }
}

/// TLS-related errors
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("Failed to load certificate: {0}")]
    CertLoad(String),

    #[error("Failed to load private key: {0}")]
    KeyLoad(String),

    #[error("TLS configuration error: {0}")]
    Config(String),

    #[error("TLS handshake failed: {0}")]
    Handshake(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_error_display() {
        let err = TlsError::CertLoad("file not found".into());
        assert!(err.to_string().contains("certificate"));
    }
}
