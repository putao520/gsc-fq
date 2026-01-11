use ring::hmac;

fn main() {
    // RFC 6238: Secret = "12345678901234567890" (ASCII)
    let secret = b"12345678901234567890";
    let counter: u64 = 1;  // T=59, floor(59/30) = 1
    let counter_bytes = counter.to_be_bytes();
    
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &counter_bytes);
    let hmac_result = tag.as_ref();
    
    let offset = (hmac_result[hmac_result.len() - 1] & 0x0f) as usize;
    let binary = ((hmac_result[offset] as u32 & 0x7f) << 24)
        | ((hmac_result[offset + 1] as u32 & 0xff) << 16)
        | ((hmac_result[offset + 2] as u32 & 0xff) << 8)
        | (hmac_result[offset + 3] as u32 & 0xff);
    
    // RFC 6238 官方值是 8 位
    let totp_8 = binary % 100_000_000;
    // 我的实现是 6 位
    let totp_6 = binary % 1_000_000;
    
    println!("RFC 6238 官方值 (8位): {}", totp_8);
    println!("我的实现值 (6位):   {}", totp_6);
    println!("RFC 期望:            94287082");
    println!("匹配? {}", totp_8 == 94287082);
    println!();
    println!("结论: 如果测试期望 948123，那可能是某个6位截断版本");
    println!("      但 RFC 6238 官方 8 位值应该是 {}", totp_8);
}
