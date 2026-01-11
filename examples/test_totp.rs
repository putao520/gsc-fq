use ring::hmac;

fn main() {
    let secret = b"12345678901234567890";
    let counter: u64 = 1;
    let counter_bytes = counter.to_be_bytes();

    println!("Secret (ASCII): {:?}", secret);
    println!("Counter: {}", counter);
    println!("Counter bytes: {:?}", counter_bytes);

    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &counter_bytes);
    let hmac_result = tag.as_ref();

    let hex_str: Vec<String> = hmac_result.iter().map(|b| format!("{:02x}", b)).collect();
    println!("HMAC-SHA1 (hex): {}", hex_str.join(""));

    // Dynamic truncation
    let offset = (hmac_result[hmac_result.len() - 1] & 0x0f) as usize;
    println!("Offset: {}", offset);
    println!("HMAC[offset..offset+4]: {:?}", &hmac_result[offset..offset+4]);

    let b1 = (hmac_result[offset] as u32 & 0x7f) << 24;
    let b2 = (hmac_result[offset + 1] as u32 & 0xff) << 16;
    let b3 = (hmac_result[offset + 2] as u32 & 0xff) << 8;
    let b4 = (hmac_result[offset + 3] as u32 & 0xff);

    let code = b1 | b2 | b3 | b4;
    println!("Binary code: {}", code);
    println!("TOTP (6 digits): {}", code % 1_000_000);
    println!("Expected: 948123");
}
