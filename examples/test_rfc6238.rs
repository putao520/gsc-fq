use ring::hmac;

fn main() {
    let secret = "12345678901234567890";
    let counter: u64 = 1;
    let counter_bytes = counter.to_be_bytes();
    
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret.as_bytes());
    let tag = hmac::sign(&key, &counter_bytes);
    let hmac_result = tag.as_ref();
    
    let hex_bytes: Vec<String> = hmac_result.iter().map(|b| format!("{:02x}", b)).collect();
    let hex_string = hex_bytes.join("");
    println!("HMAC-SHA1: {}", hex_string);
    
    let offset = (hmac_result[hmac_result.len() - 1] & 0x0f) as usize;
    let code = ((hmac_result[offset] as u32 & 0x7f) << 24)
        | ((hmac_result[offset + 1] as u32 & 0xff) << 16)
        | ((hmac_result[offset + 2] as u32 & 0xff) << 8)
        | (hmac_result[offset + 3] as u32 & 0xff);
    
    println!("My result: {}, Expected: 948123", code % 1_000_000);
}
