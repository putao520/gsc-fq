/// Linux splice() 零拷贝优化器
///
/// 专门用于 Socket 到 Socket 的高性能零拷贝传输
/// 仅在 Linux 上可用，自动检测并使用 splice() 系统调用

#[cfg(target_os = "linux")]
use crate::debug_println;
#[cfg(target_os = "linux")]
use crate::error::{ProxyError, Result};
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, RawFd};

use tokio::io::{AsyncRead, AsyncWrite};

/// splice() 零拷贝传输
///
/// 直接在内核空间将数据从一个 fd 传输到另一个 fd，完全零拷贝
/// 仅适用于文件描述符（如 Socket、Pipe、文件等）
#[cfg(target_os = "linux")]
pub async fn splice_zero_copy(
    mut reader: impl AsyncRead + Unpin + AsRawFd,
    mut writer: impl AsyncWrite + Unpin + AsRawFd,
) -> Result<u64> {
    use std::os::fd::AsRawFd;
    use tokio::task::spawn_blocking;

    debug_println!("🚀 使用 splice() 零拷贝传输");

    // 获取文件描述符
    let reader_fd = reader.as_raw_fd();
    let writer_fd = writer.as_raw_fd();

    // 在阻塞线程中执行 splice()（因为它是同步系统调用）
    spawn_blocking(move || {
        splice_blocking(reader_fd, writer_fd)
    })
    .await
    .map_err(|e| ProxyError::ForwardingFailed(format!("Splice task failed: {}", e)))?
}

#[cfg(target_os = "linux")]
fn splice_blocking(reader_fd: RawFd, writer_fd: RawFd) -> Result<u64> {
    use libc::size_t;
    use std::ptr;

    const SPLICE_F_MOVE: u32 = 1;     // 移动页而不是复制
    const SPLICE_F_NONBLOCK: u32 = 2; // 非阻塞模式
    const BUFFER_SIZE: usize = 256 * 1024; // 256KB splice 块

    // 创建管道用于中转
    let mut pipe_fds = [0i32; 2];
    unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            return Err(ProxyError::ForwardingFailed(format!(
                "Pipe creation failed: {}",
                std::io::Error::last_os_error()
            ))
            .into());
        }

        // 设置管道为非阻塞
        libc::fcntl(pipe_fds[0], libc::F_SETFL, libc::O_NONBLOCK);
        libc::fcntl(pipe_fds[1], libc::F_SETFL, libc::O_NONBLOCK);
    }

    let pipe_read = pipe_fds[0];
    let pipe_write = pipe_fds[1];

    let mut total = 0u64;

    loop {
        // 步骤 1: reader → pipe
        let n = unsafe {
            libc::splice(
                reader_fd,
                ptr::null_mut(),
                pipe_write,
                ptr::null_mut(),
                BUFFER_SIZE as size_t,
                SPLICE_F_MOVE | SPLICE_F_NONBLOCK,
            )
        };

        match n {
            n if n > 0 => {
                // 成功读取 n 字节到管道
                let mut remaining = n as usize;

                // 步骤 2: pipe → writer
                while remaining > 0 {
                    let written = unsafe {
                        libc::splice(
                            pipe_read,
                            ptr::null_mut(),
                            writer_fd,
                            ptr::null_mut(),
                            remaining as size_t,
                            SPLICE_F_MOVE | SPLICE_F_NONBLOCK,
                        )
                    };

                    match written {
                        w if w > 0 => {
                            remaining -= w as usize;
                            total += w as u64;
                        }
                        0 => {
                            debug_println!("⚠️  splice() 返回 0（pipe → writer）");
                            break;
                        }
                        _ => {
                            let err = std::io::Error::last_os_error();
                            if err.kind() == std::io::ErrorKind::WouldBlock {
                                // 非阻塞模式下需要等待
                                std::thread::sleep(std::time::Duration::from_micros(100));
                                continue;
                            } else if err.kind() == std::io::ErrorKind::Interrupted {
                                // 被信号中断，重试
                                continue;
                            } else {
                                // 其他错误，关闭管道并返回
                                unsafe {
                                    libc::close(pipe_read);
                                    libc::close(pipe_write);
                                }
                                return Err(ProxyError::ForwardingFailed(format!(
                                    "Splice pipe→writer failed: {}",
                                    err
                                ))
                                .into());
                            }
                        }
                    }
                }
            }
            0 => {
                // EOF
                debug_println!("✅ splice() 完成，总计 {} 字节", total);
                break;
            }
            _ => {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    // 非阻塞模式下没有数据可读
                    std::thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                } else if err.kind() == std::io::ErrorKind::Interrupted {
                    // 被信号中断，重试
                    continue;
                } else {
                    // 其他错误
                    unsafe {
                        libc::close(pipe_read);
                        libc::close(pipe_write);
                    }
                    return Err(ProxyError::ForwardingFailed(format!(
                        "Splice reader→pipe failed: {}",
                        err
                    ))
                    .into());
                }
            }
        }
    }

    // 关闭管道
    unsafe {
        libc::close(pipe_read);
        libc::close(pipe_write);
    }

    Ok(total)
}

/// 检查流是否支持 splice()
///
/// splice() 只支持特定类型的文件描述符：
/// - Socket (AF_INET, AF_INET6, AF_UNIX)
/// - Pipe
/// - 普通文件
/// - 不支持: Terminal, 设备文件等
#[cfg(target_os = "linux")]
pub fn supports_splice<T: AsRawFd>(stream: &T) -> bool {
    use libc::{S_IFIFO, S_IFMT, S_IFSOCK};
    use std::os::fd::AsRawFd;

    let fd = stream.as_raw_fd();

    unsafe {
        let mut stat = std::mem::zeroed::<libc::stat>();
        if libc::fstat(fd, &mut stat) != 0 {
            return false;
        }

        let st_mode = stat.st_mode & S_IFMT;
        // Socket 或 Pipe 支持 splice()
        st_mode == S_IFSOCK || st_mode == S_IFIFO
    }
}

/// 优化的 splice() 传输（带缓冲区大小调整）
///
/// 根据数据量动态调整 splice 块大小
#[cfg(target_os = "linux")]
pub async fn splice_adaptive(
    mut reader: impl AsyncRead + Unpin + AsRawFd,
    mut writer: impl AsyncWrite + Unpin + AsRawFd,
    initial_sample_size: usize,
) -> Result<u64> {
    use std::os::fd::AsRawFd;
    use tokio::task::spawn_blocking;

    // 基于初始采样选择块大小
    let chunk_size = if initial_sample_size < 64 * 1024 {
        64 * 1024 // 64KB for small data
    } else if initial_sample_size < 1024 * 1024 {
        128 * 1024 // 128KB for medium data
    } else {
        256 * 1024 // 256KB for large data
    };

    debug_println!("🚀 使用 splice() 自适应零拷贝 (块大小: {})", format_size(chunk_size));

    let reader_fd = reader.as_raw_fd();
    let writer_fd = writer.as_raw_fd();

    spawn_blocking(move || {
        splice_adaptive_blocking(reader_fd, writer_fd, chunk_size)
    })
    .await
    .map_err(|e| ProxyError::ForwardingFailed(format!("Splice task failed: {}", e)))?
}

#[cfg(target_os = "linux")]
fn splice_adaptive_blocking(
    reader_fd: RawFd,
    writer_fd: RawFd,
    chunk_size: usize,
) -> Result<u64> {
    use libc::size_t;
    use std::ptr;

    const SPLICE_F_MOVE: u32 = 1;
    const SPLICE_F_NONBLOCK: u32 = 2;

    let mut pipe_fds = [0i32; 2];
    unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            return Err(ProxyError::ForwardingFailed(format!(
                "Pipe creation failed: {}",
                std::io::Error::last_os_error()
            ))
            .into());
        }

        libc::fcntl(pipe_fds[0], libc::F_SETFL, libc::O_NONBLOCK);
        libc::fcntl(pipe_fds[1], libc::F_SETFL, libc::O_NONBLOCK);
    }

    let pipe_read = pipe_fds[0];
    let pipe_write = pipe_fds[1];
    let mut total = 0u64;

    loop {
        let n = unsafe {
            libc::splice(
                reader_fd,
                ptr::null_mut(),
                pipe_write,
                ptr::null_mut(),
                chunk_size as size_t,
                SPLICE_F_MOVE | SPLICE_F_NONBLOCK,
            )
        };

        match n {
            n if n > 0 => {
                let mut remaining = n as usize;
                while remaining > 0 {
                    let written = unsafe {
                        libc::splice(
                            pipe_read,
                            ptr::null_mut(),
                            writer_fd,
                            ptr::null_mut(),
                            remaining as size_t,
                            SPLICE_F_MOVE | SPLICE_F_NONBLOCK,
                        )
                    };

                    match written {
                        w if w > 0 => {
                            remaining -= w as usize;
                            total += w as u64;
                        }
                        0 => break,
                        _ => {
                            let err = std::io::Error::last_os_error();
                            if err.kind() == std::io::ErrorKind::WouldBlock {
                                std::thread::sleep(std::time::Duration::from_micros(100));
                                continue;
                            } else if err.kind() == std::io::ErrorKind::Interrupted {
                                continue;
                            } else {
                                unsafe {
                                    libc::close(pipe_read);
                                    libc::close(pipe_write);
                                }
                                return Err(ProxyError::ForwardingFailed(format!(
                                    "Splice failed: {}",
                                    err
                                ))
                                .into());
                            }
                        }
                    }
                }
            }
            0 => break,
            _ => {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                } else if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                } else {
                    unsafe {
                        libc::close(pipe_read);
                        libc::close(pipe_write);
                    }
                    return Err(ProxyError::ForwardingFailed(format!(
                        "Splice failed: {}",
                        err
                    ))
                    .into());
                }
            }
        }
    }

    unsafe {
        libc::close(pipe_read);
        libc::close(pipe_write);
    }

    Ok(total)
}

#[cfg(target_os = "linux")]
fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{}MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_size() {
        assert_eq!(format_size(1024), "1KB");
        assert_eq!(format_size(1024 * 1024), "1MB");
        assert_eq!(format_size(512), "512B");
    }
}
