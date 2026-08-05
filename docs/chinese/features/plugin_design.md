<!-- vim-markdown-toc GFM -->

* [插件设计](#插件设计)
    * [概述](#概述)
    * [架构](#架构)
    * [插件 Trait](#插件-trait)
    * [IPC 协议](#ipc-协议)
    * [插件发现与生命周期](#插件发现与生命周期)
    * [守护进程侧插件管理](#守护进程侧插件管理)
    * [实现一个插件](#实现一个插件)

<!-- vim-markdown-toc -->

# 插件设计

## 概述

Nipart 支持进程外插件系统。每个插件作为独立的二进制进程运行，通过 Unix
域套接字与 nipart 守护进程通信。这种架构提供了进程隔离：插件崩溃不会导致
守护进程崩溃，插件可以独立开发、测试和升级。

目前 Nipart 内置以下插件：

- **nipart-plugin-wifi**：通过 wpa_supplicant 管理 WiFi 连接
- **nipart-plugin-ovs**：管理 Open vSwitch 网桥和端口

同时提供了 `nipart-plugin-demo` 作为参考实现。

## 架构

```
┌─────────────┐    Unix Socket     ┌─────────────────────┐
│             │◄──────────────────►│ nipart-plugin-wifi   │
│             │  /var/run/nipart/  └─────────────────────┘
│   nipart    │  sockets/plugin/
│   守护进程   │                    ┌─────────────────────┐
│             │◄──────────────────►│ nipart-plugin-ovs    │
│             │                    └─────────────────────┘
└─────────────┘
```

每个插件都是独立进程。守护进程在启动时发现插件，将其作为子进程启动，并通过
位于 `/var/run/nipart/sockets/plugin/<插件名>` 的 Unix 域套接字进行通信。

## 插件 Trait

所有插件实现 `NipartPlugin` trait，定义于 `src/lib/plugin/plugin_trait.rs`：

```rust
pub trait NipartPlugin: Send + Sync + Sized + 'static {
    const PLUGIN_NAME: &'static str;

    fn init() -> impl Future<Output = Result<Self, NipartError>> + Send;
    fn plugin_info(plugin: &Arc<Self>) -> impl Future<Output = Result<NipartPluginInfo, NipartError>> + Send;
    fn query_network_state(...) -> impl Future<Output = Result<NetworkState, NipartError>> + Send;
    fn apply_network_state(...) -> impl Future<Output = Result<(), NipartError>> + Send;
    fn wifi_scan(...) -> impl Future<Output = Result<Vec<WifiScanResult>, NipartError>> + Send;
    fn quit(plugin: &Arc<Self>) -> impl Future<Output = ()> + Send;
}
```

### 必需方法

| 方法 | 说明 |
|------|------|
| `PLUGIN_NAME` | 标识插件的常量字符串（如 `"wifi"`、`"ovs"`） |
| `init()` | 插件初始化，在插件进程启动时调用一次 |
| `plugin_info()` | 返回 `NipartPluginInfo`，包含插件名称、版本和支持的接口类型 |

### 可选方法（带默认"不支持"实现）

| 方法 | 说明 |
|------|------|
| `query_network_state()` | 查询此插件管理的网络状态。接收当前内核状态（通过 nispor 查询）作为参考 |
| `apply_network_state()` | 将期望的网络状态应用到托管子系统 |
| `wifi_scan()` | 执行主动 WiFi 扫描并返回发现的网络 |
| `quit()` | 优雅退出。默认实现调用 `std::process::exit(0)` |

trait 提供的默认 `run()` 方法处理整个插件生命周期：调用 `init()`，创建 Unix
套接字监听器，然后循环接受来自守护进程的连接，为每个连接创建一个 tokio 任务。

## IPC 协议

守护进程与插件之间的通信使用基于 Unix 域套接字的结构化命令/响应协议。
每条消息通过 serde 序列化。

### 命令（守护进程 → 插件）

`NipartPluginCmd` 枚举定义了守护进程可发送的所有命令：

| 命令 | 参数 | 期望响应 |
|------|------|----------|
| `QueryPluginInfo` | 无 | `NipartPluginInfo` |
| `QueryNetworkState` | `(NipartQueryOption, NetworkState)` | `NetworkState` |
| `ApplyNetworkState` | `(NetworkState, NipartApplyOption)` | `()` |
| `WifiScan` | `NipartWifiScanOption` | `Vec<WifiScanResult>` |
| `Quit` | 无 | 无（进程退出） |

### 插件信息

`NipartPluginInfo` 结构体标识插件的功能：

```rust
pub struct NipartPluginInfo {
    pub name: String,              // 插件名称，如 "wifi"
    pub version: String,           // 插件版本，如 "0.1.0"
    pub iface_types: Vec<InterfaceType>,  // 此插件管理的接口类型
}
```

`iface_types` 字段至关重要——它告诉守护进程此插件处理哪些接口类型。
应用状态时，守护进程根据 `iface_type()` 过滤接口，仅将匹配的接口路由到
相应的插件。

### 插件日志

插件可以通过连接对象（`conn.log_trace()`、`conn.log_debug()` 等）将日志
消息发送回守护进程，使插件特定的诊断信息出现在守护进程的日志输出中。

## 插件发现与生命周期

### 发现

启动时，守护进程的插件工作线程扫描守护进程二进制文件所在目录，查找名称以
`nipart-plugin-` 开头的可执行文件。例如，若守护进程位于 `/usr/bin/nipart`，
则查找 `/usr/bin/nipart-plugin-*`。

### 启动

1. 每个发现的插件二进制文件作为子进程启动
2. 插件调用 `init()` 并开始在 Unix 套接字
   `/var/run/nipart/sockets/plugin/<PLUGIN_NAME>` 上监听
3. 守护进程重试连接每个插件套接字（最多 50 次，间隔 200ms，总计约 10 秒超时）
4. 连接成功后，守护进程发送 `QueryPluginInfo` 注册插件并获取其功能信息

### 关闭

守护进程关闭时：
- 每个插件子进程收到 `SIGKILL`
- 插件 Unix 套接字被清理

### 插件进程命名

插件二进制文件遵循命名约定 `nipart-plugin-<名称>`：

- `nipart-plugin-wifi`
- `nipart-plugin-ovs`
- `nipart-plugin-demo`（守护进程在正常操作中忽略）

## 守护进程侧插件管理

守护进程的插件子系统位于 `src/daemon/plugin/`，由三个组件组成：

### `NipartPluginWorker`

实现 `TaskWorker` trait。这是主要的插件编排器，负责：
- 发现并启动插件进程
- 维护已连接插件的注册表（`HashMap<String, NipartDaemonPlugin>`）
- 追踪所有插件支持的接口类型
- 将 `QueryNetworkState`、`ApplyNetworkState` 和 `WifiScan` 命令分发到
  相应的插件
- 优雅处理插件故障（记录错误，不使守护进程崩溃）

### `NipartPluginManager`

对 `TaskManager<NipartPluginCmd, NipartPluginReply>` 的薄封装，为守护进程
其他部分提供简洁的异步 API 来与插件交互。

### `NipartDaemonPlugin`

从守护进程视角表示一个已连接的插件。持有插件的名称、信息和套接字路径。
提供便捷方法（`query_network_state`、`apply_network_state`、`wifi_scan`），
用于创建 IPC 客户端、发送命令并返回响应。

### 接口类型路由

应用状态时，每个插件只接收与其声明的 `iface_types` 匹配的接口。例如，
`nipart-plugin-wifi` 只接收类型为 `WifiCfg` 或 `WifiPhy` 的接口。如果期望
状态中的某个接口类型没有任何插件声明支持，守护进程返回 `DependencyError`。

## 实现一个插件

### 项目结构

插件是 `src/plugin-<名称>/` 下的独立 Rust 二进制 crate：

```
src/plugin-wifi/
├── Cargo.toml
├── main.rs        # 入口点：调用 NipartPlugin::run()
├── plugin.rs      # NipartPlugin trait 实现
├── apply.rs       # 应用逻辑
├── query.rs       # 查询逻辑
├── scan.rs        # WiFi 扫描逻辑
└── ...
```

### 最小插件示例

```rust
use nipart::{NipartPlugin, NipartPluginInfo, NipartError, NetworkState,
    NipartQueryOption, NipartApplyOption, NipartIpcConnection};
use std::sync::Arc;

struct MyPlugin;

impl NipartPlugin for MyPlugin {
    const PLUGIN_NAME: &'static str = "my-plugin";

    async fn init() -> Result<Self, NipartError> {
        Ok(Self {})
    }

    async fn plugin_info(_plugin: &Arc<Self>) -> Result<NipartPluginInfo, NipartError> {
        Ok(NipartPluginInfo::new(
            "my-plugin".to_string(),
            "0.1.0".to_string(),
            vec![],
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), NipartError> {
    MyPlugin::run().await
}
```

### Cargo.toml 要求

```toml
[package]
name = "nipart-plugin-my-plugin"
version = "0.1.0"
edition = "2024"

[dependencies]
nipart = { path = "../lib" }
tokio = { version = "1", features = ["full"] }
```

### 关键实现说明

1. **插件二进制名称必须以 `nipart-plugin-` 开头**，守护进程才能发现它
2. **`PLUGIN_NAME` 决定套接字路径**：守护进程连接到
   `/var/run/nipart/sockets/plugin/<PLUGIN_NAME>`
3. **接口类型路由**：在 `plugin_info()` 中声明插件管理的接口类型。
   守护进程只会发送这些类型的接口
4. **连接处理**：每个守护进程连接生成一个独立的 tokio 任务；
   使用 `Arc<Self>` 共享插件状态
5. **错误处理**：失败时返回带有适当 `ErrorKind` 的 `NipartError`；
   守护进程记录插件错误但继续运行
6. **日志**：使用 `conn.log_trace()` / `conn.log_debug()` 将日志消息
   发送到守护进程的日志流
7. **demo 插件自动被忽略**，无需特殊配置即可排除
