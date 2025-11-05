use std::sync::atomic::{AtomicBool, Ordering};

/// 全局调试开关控制
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// 初始化调试系统
pub fn init_debug(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::Relaxed);

    if enabled {
        // 初始化 env_logger 用于结构化日志
        std::env::set_var("RUST_LOG", "debug");
        env_logger::init();
    }
}

/// 检查是否启用了调试模式
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// 调试输出宏 - 只在调试模式下输出
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        if $crate::utils::debug::is_debug_enabled() {
            println!($($arg)*);
            // 只在调试模式下刷新输出缓冲区
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    };
}

/// 错误输出宏 - 始终输出（错误信息不应该被禁用）
#[macro_export]
macro_rules! error_println {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
        use std::io::Write;
        let _ = std::io::stderr().flush();
    };
}

/// 调试格式化输出宏
#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        if $crate::utils::debug::is_debug_enabled() {
            print!($($arg)*);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    };
}
