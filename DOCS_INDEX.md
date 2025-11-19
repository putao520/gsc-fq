# GSC-FQ 文档索引

本文档索引帮助您快速找到所需的信息。

## 📚 文档列表

### 项目规范文档

| 文档 | 描述 | 适合对象 |
|-----|-----|--------|
| [PROJECT_SPEC.md](PROJECT_SPEC.md) | 项目总体规范和功能定义 | 项目经理、开发者 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 详细的技术架构设计 | 开发者、架构师 |
| [API.md](API.md) | 公共API文档和使用示例 | 开发者 |
| [TESTING.md](TESTING.md) | 测试指南和最佳实践 | QA、开发者 |
| [DEPLOYMENT.md](DEPLOYMENT.md) | 部署、运维和监控指南 | 运维人员、部署工程师 |
| [README.md](README.md) | 项目快速开始指南 | 所有用户 |
| [README.docker.md](README.docker.md) | Docker部署指南 | 容器部署人员 |

## 🎯 快速导航

### 对于新开发者

1. 首先阅读 [README.md](README.md) - 了解项目概况
2. 然后阅读 [PROJECT_SPEC.md](PROJECT_SPEC.md) - 理解项目功能
3. 再阅读 [ARCHITECTURE.md](ARCHITECTURE.md) - 了解代码结构
4. 查看 [API.md](API.md) - 学习如何使用库

### 对于维护人员

1. 阅读 [DEPLOYMENT.md](DEPLOYMENT.md) - 学习部署方式
2. 查看 [TESTING.md](TESTING.md) - 了解测试流程
3. 参考 [ARCHITECTURE.md](ARCHITECTURE.md) - 理解代码变更的影响

### 对于贡献者

1. 查看 [PROJECT_SPEC.md](PROJECT_SPEC.md) - 确认功能需求
2. 阅读 [ARCHITECTURE.md](ARCHITECTURE.md) - 了解设计决策
3. 参考 [TESTING.md](TESTING.md) - 编写合适的测试
4. 使用 [API.md](API.md) - 实现标准化的接口

## 📖 文档内容概览

### PROJECT_SPEC.md

**内容**：
- 项目概述和定位
- 核心功能说明
- 系统需求
- 编译和发布
- 测试和质量
- 错误处理
- 安全考虑
- 使用场景
- 架构设计
- 开发工作流

**主要章节**：18个

**适用场景**：
- 项目评审
- 功能规划
- 需求验证
- 新成员入职

### ARCHITECTURE.md

**内容**：
- 架构概述和设计原则
- 模块架构详解
- 数据流图
- 并发模型
- 内存管理
- 错误恢复
- 性能优化
- 配置系统
- 协议设计
- 扩展性

**主要章节**：11个

**适用场景**：
- 代码审查
- 架构设计
- 性能优化
- 功能扩展

### API.md

**内容**：
- 公共API说明
- 各个模块的API文档
- 完整使用示例
- 最佳实践
- 安全建议

**主要内容**：
- ConfigLoader API
- ProxyServerBuilder API
- ReverseProxyServer API
- ReverseProxyClient API
- Error 类型

**适用场景**：
- 集成开发
- 库使用
- 第三方开发

### TESTING.md

**内容**：
- 测试概述
- 单元测试指南
- 集成测试指南
- 基准测试
- 代码覆盖率
- 测试工作流
- 故障排查
- CI/CD 集成

**主要章节**：10个

**适用场景**：
- 测试开发
- 质量保证
- 代码审查
- CI/CD 配置

### DEPLOYMENT.md

**内容**：
- 部署前准备
- 构建和安装
- 配置管理
- Systemd 配置
- Docker 部署
- 监控和日志
- 备份和恢复
- 升级流程
- 故障排查
- 性能调优

**主要章节**：12个

**适用场景**：
- 系统部署
- 运维管理
- 监控配置
- 故障排查
- 性能优化

## 🔍 按任务查找

### 我想...

**...安装GSC-FQ**
- 参考: [README.md](README.md) - Installation 部分
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第2节 构建和安装

**...配置GSC-FQ**
- 参考: [README.md](README.md) - Quick Start 部分
- 参考: [PROJECT_SPEC.md](PROJECT_SPEC.md) - 第4节 配置系统

**...部署到生产环境**
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第4节 Systemd 服务配置
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第5节 Docker 部署

**...编写单元测试**
- 参考: [TESTING.md](TESTING.md) - 第2节 单元测试

**...监控运行状态**
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第6节 监控和日志

**...修复运行问题**
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第9节 故障排查

**...优化性能**
- 参考: [DEPLOYMENT.md](DEPLOYMENT.md) - 第11节 性能调优
- 参考: [ARCHITECTURE.md](ARCHITECTURE.md) - 第7节 性能优化

**...扩展功能**
- 参考: [ARCHITECTURE.md](ARCHITECTURE.md) - 第10节 扩展性
- 参考: [API.md](API.md) - 完整使用示例

**...理解代码结构**
- 参考: [ARCHITECTURE.md](ARCHITECTURE.md) - 第2节 模块架构
- 参考: [PROJECT_SPEC.md](PROJECT_SPEC.md) - 第3节 架构设计

**...集成GSC-FQ库**
- 参考: [API.md](API.md) - 第7节 完整使用示例

## 📋 文档版本和更新

| 文档 | 版本 | 最后更新 |
|-----|-----|--------|
| PROJECT_SPEC.md | 1.0 | 2024年11月 |
| ARCHITECTURE.md | 1.0 | 2024年11月 |
| API.md | 1.0 | 2024年11月 |
| TESTING.md | 1.0 | 2024年11月 |
| DEPLOYMENT.md | 1.0 | 2024年11月 |

## 🔗 关联资源

- **主仓库**: https://github.com/putao520/gsc-fq
- **Crates.io**: https://crates.io/crates/gsc-fq
- **文档**: https://docs.rs/gsc-fq
- **许可证**: MIT OR Apache-2.0

## 💡 使用建议

1. **首次使用**: 按照顺序阅读 README → PROJECT_SPEC → ARCHITECTURE
2. **日常开发**: 参考 API.md 和 TESTING.md
3. **部署运维**: 主要参考 DEPLOYMENT.md
4. **问题排查**: 查看 DEPLOYMENT.md 中的故障排查部分

## ❓ 常见问题

**Q: 文档和代码不一致怎么办？**
A: 优先信任代码。如果发现不一致，请提交Issue。

**Q: 文档中的示例无法运行？**
A: 确保您已正确安装Rust和所有依赖。参考DEPLOYMENT.md的环境要求部分。

**Q: 如何提出文档改进建议？**
A: 提交Pull Request或Issue到GitHub仓库。

**Q: 文档是否支持其他语言？**
A: 目前仅提供英文和中文。欢迎贡献翻译。

## 🤝 贡献指南

如果您发现文档错误或有改进建议：

1. Fork 项目仓库
2. 创建特性分支 (`git checkout -b docs/improvement`)
3. 提交更改 (`git commit -am 'Improve documentation'`)
4. 推送到分支 (`git push origin docs/improvement`)
5. 创建 Pull Request

## 📝 文档维护

所有文档由项目维护者维护，确保与代码和功能同步更新。

---

**索引版本**: 1.0  
**最后更新**: 2024年11月  
**维护者**: Claude Code AI
