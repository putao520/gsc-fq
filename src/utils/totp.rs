use data_encoding::BASE32;
use qrcode::QrCode;
use ring::hmac;
use std::time::{SystemTime, UNIX_EPOCH};

/// TOTP (Time-based One-Time Password) implementation (RFC 6238)
pub struct Totp {
    secret: Vec<u8>,
    digits: u32,
    step_secs: u64,
}

impl Totp {
    pub fn new(secret: Vec<u8>) -> Self {
        Self {
            secret,
            digits: 6,
            step_secs: 30,
        }
    }

    /// Create TOTP with custom number of digits (for RFC 6238 compliance testing)
    pub fn with_digits(secret: Vec<u8>, digits: u32) -> Self {
        Self {
            secret,
            digits,
            step_secs: 30,
        }
    }

    /// Create from Base32 string (standard Google Authenticator format)
    pub fn from_base32(secret_b32: &str) -> std::result::Result<Self, String> {
        let mut clean_secret = secret_b32.replace(" ", "").to_uppercase();

        // Add padding if needed (data_encoding requires proper padding)
        while clean_secret.len() % 8 != 0 {
            clean_secret.push('=');
        }

        let bytes = BASE32
            .decode(clean_secret.as_bytes())
            .map_err(|e| format!("Invalid Base32 secret: {}", e))?;
        Ok(Self::new(bytes))
    }

    /// Generate Google Authenticator compatible URI
    pub fn generate_otpauth_uri(&self, label: &str, issuer: &str) -> String {
        let secret_b32 = BASE32.encode(&self.secret);
        format!(
            "otpauth://totp/{}?secret={}&issuer={}&digits={}&period={}",
            label, secret_b32, issuer, self.digits, self.step_secs
        )
    }

    /// Generate a random 16-byte secret and return both Totp and Base32 string
    pub fn generate_random() -> (Self, String) {
        use rand::RngCore;
        let mut secret = [0u8; 10]; // 80 bits is typical for Google Authenticator (16 chars in B32)
        rand::rng().fill_bytes(&mut secret);
        let secret_b32 = BASE32.encode(&secret);
        (Self::new(secret.to_vec()), secret_b32)
    }

    /// Render QR code as terminal string
    pub fn render_qr_code(&self, uri: &str) -> String {
        let code = QrCode::new(uri.as_bytes()).unwrap();
        let string = code
            .render::<char>()
            .quiet_zone(true)
            .module_dimensions(2, 1)
            .build();
        string
    }

    /// Generate TOTP code for the current time
    pub fn generate_current(&self) -> u32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.generate(now)
    }

    /// Generate TOTP code for a specific timestamp
    pub fn generate(&self, timestamp: u64) -> u32 {
        let counter = timestamp / self.step_secs;
        let counter_bytes = counter.to_be_bytes();

        let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &self.secret);
        let tag = hmac::sign(&key, &counter_bytes);
        let hmac_result = tag.as_ref();

        let offset = (hmac_result[hmac_result.len() - 1] & 0x0f) as usize;
        let b1 = (hmac_result[offset] as u32 & 0x7f) << 24;
        let b2 = (hmac_result[offset + 1] as u32 & 0xff) << 16;
        let b3 = (hmac_result[offset + 2] as u32 & 0xff) << 8;
        let b4 = hmac_result[offset + 3] as u32 & 0xff;

        let code = b1 | b2 | b3 | b4;
        code % 10u32.pow(self.digits)
    }

    /// Verify TOTP code with time window drift (±1 step)
    pub fn verify(&self, code: u32) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check current, previous, and next windows
        for i in -1..=1 {
            let ts = if i < 0 {
                now.saturating_sub(self.step_secs)
            } else if i > 0 {
                now.saturating_add(self.step_secs)
            } else {
                now
            };

            if self.generate(ts) == code {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_totp_generation_and_verification() {
        let secret = b"12345678901234567890".to_vec();
        let totp = Totp::new(secret);

        let code = totp.generate_current();
        assert!(totp.verify(code));

        // Verify with fake code fails
        assert!(!totp.verify(999999));
    }

    #[test]
    fn test_rfc_6238_vectors() {
        // RFC 6238 Appendix B - Test Vectors
        // Secret = "12345678901234567890" (ASCII string, 20 bytes)
        // Time Step X = 30, T0 = 0 (Unix epoch)
        // Reference: https://datatracker.ietf.org/doc/html/rfc6238#appendix-B

        let secret = b"12345678901234567890".to_vec();

        // RFC 6238 test vectors use 8-digit codes
        let totp = Totp::with_digits(secret.clone(), 8);

        // Table 1, Row 1: T=59, T(Hex)=0000000000000001, TOTP=94287082 (SHA1)
        assert_eq!(totp.generate(59), 94287082,
                   "RFC 6238: T=59 should produce 94287082 (8-digit SHA1)");

        // Table 1, Row 4: T=1111111109, T(Hex)=00000000023523EC, TOTP=07081804 (SHA1)
        assert_eq!(totp.generate(1111111109), 7081804,
                   "RFC 6238: T=1111111109 should produce 07081804 (8-digit SHA1)");

        // Table 1, Row 5: T=1111111111, T(Hex)=00000000023523ED, TOTP=14050471 (SHA1)
        assert_eq!(totp.generate(1111111111), 14050471,
                   "RFC 6238: T=1111111111 should produce 14050471 (8-digit SHA1)");

        // Table 1, Row 6: T=1234567890, T(Hex)=000000000273EF07, TOTP=89005924 (SHA1)
        assert_eq!(totp.generate(1234567890), 89005924,
                   "RFC 6238: T=1234567890 should produce 89005924 (8-digit SHA1)");

        // Verify 6-digit mode also works (standard for Google Authenticator)
        let totp_6 = Totp::with_digits(secret, 6);
        assert_eq!(totp_6.generate(59), 287082,
                   "6-digit mode: T=59 should produce 287082 (last 6 digits of 94287082)");
    }

    #[test]
    fn test_base32_and_uri() {
        // Test with proper Base32 encoding
        let hello = b"Hello!";
        let b32_secret = BASE32.encode(hello);
        println!("'Hello!' encoded as Base32: {}", b32_secret);

        let totp = Totp::from_base32(&b32_secret).unwrap();
        assert_eq!(totp.secret, hello);

        let uri = totp.generate_otpauth_uri("test@example.com", "GSC-FQ");
        assert!(uri.contains(&format!("secret={}", b32_secret)));
        assert!(uri.contains("issuer=GSC-FQ"));
    }
}
