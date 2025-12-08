fn main() {
    // 交叉编译时的配置
    let target = std::env::var("TARGET").unwrap_or_default();

    // 针对不同架构的优化配置
    match target.as_str() {
        // ARM 架构的特定配置
        "arm-unknown-linux-musleabihf" => {
            println!("cargo:rustc-cfg=arm_arch");
            println!("cargo:rustc-link-arg=-specs");
            println!("cargo:rustc-link-arg=/usr/lib/arm-linux-gnueabihf/musl-gcc.specs");
        }
        "aarch64-unknown-linux-musl" => {
            println!("cargo:rustc-cfg=aarch64_arch");
        }
        "x86_64-unknown-linux-musl" => {
            println!("cargo:rustc-cfg=x86_64_arch");
        }
        _ => {
            // 其他架构的默认配置
        }
    }

    // 在 release 模式下启用优化
    if std::env::var("PROFILE").unwrap_or_default() == "release" {
        println!("cargo:rustc-link-arg=-s");
    }
}