<!-- vim-markdown-toc GFM -->

* [WIFI 快速启动（提示信息）](#wifi-快速启动提示信息)
    * [背景与动机](#背景与动机)
    * [目标](#目标)
    * [非目标](#非目标)
    * [设计概览](#设计概览)
    * [提示文件](#提示文件)
        * [位置与文件名](#位置与文件名)
        * [文件格式](#文件格式)
        * [校验](#校验)
        * [写入路径](#写入路径)
        * [读取路径](#读取路径)
        * [生命周期与清理](#生命周期与清理)
        * [原子性](#原子性)
    * [将提示附加到 shuli](#将提示附加到-shuli)
        * [为何不使用无扫描提示](#为何不使用无扫描提示)
        * [已提示 BSS 选择](#已提示-bss-选择)
        * [连接后最优 BSS 扫描](#连接后最优-bss-扫描)
    * [安全性](#安全性)
    * [故障处理](#故障处理)
    * [与接口监视器和启动激活的交互](#与接口监视器和启动激活的交互)
    * [测试](#测试)
    * [后续工作](#后续工作)

<!-- vim-markdown-toc -->

# WIFI 快速启动（提示信息）

本文描述 WIFI 快速启动设计：记住某个已保存 WIFI 配置曾经成功连接过的
BSS，并利用这些信息收窄启动或插件重启后的首次扫描范围。它取代了英文
设计文档 plan.md 里 `/var/lib/nipart/wifi_quick_boot.yml` 的草案。

实现：nipart 侧位于 `src/plugin-wifi/hints.rs` 以及
`src/plugin-wifi/apply.rs` 中的接入代码；`KnownBss` 提示存储、已提示
BSS 选择与连接后扫描位于 `shuli` crate（`config.rs`、`scan.rs`、
`roam.rs`、`client/wifi_iface.rs`）。

## 背景与动机

WIFI 冷启动在 shuli 选定 AP 之前需要一次全信道扫描。扫描是启动路径中
最慢的环节，而跳过它所需的大部分信息在上一轮运行中已经知道：本机曾经
加入过某个 SSID 的哪些 BSS，它们位于哪些信道。

因此 nipart 会把每次成功连接的 BSSID 与频率记录到
`/var/lib/nipart/wifi/` 下以 SSID 为键的文件中。在下一次应用
（启动交接、守护进程重启、`npt up`）时，保存的频率与 BSSID 会作为扫描
提示附加到 shuli 的网络配置上；shuli 把首次扫描限制在这些信道上，连接
到可见的、信号最强的已知 BSS；若没有可见的已知 BSS 则回退全扫描；
在连接到支持受管漫游的 BSS 后，再执行一次性扫描寻找更优 BSS。

## 目标

* 缩短曾经连接过的配置在启动/插件启动后的首次扫描时间。
* 绝不降低连通性：过期、缺失或损坏的提示最多只多花一次快速扫描，
  绝不能导致启动失败。
* 提示文件中不包含凭据，文件保持小巧且极少写入。
* 让 shuli 使用保存的 BSS（而不仅是信道）：收窄扫描后连接到信号最强的
  已提示 BSS；当连接的 BSS 支持漫游时，触发一次扫描以寻找更优 BSS。

## 非目标

* 无扫描直接认证（见[为何不使用无扫描提示](#为何不使用无扫描提示)）。

## 设计概览

```
连接成功（WifiClientState::run_once，WifiState::Connected*）
    client.export_hints(iface_name) -> bssid + frequency_mhz
    hints::record_bss(ssid, bssid, frequency_mhz)
        已知该 BSS 且今天的记录已存在时跳过写入
        /var/lib/nipart/wifi/<编码后的 SSID>.hints   (0600，原子重命名)

应用（启动交接、守护进程重启、npt up 等）
    WifiClientState::apply()
        对每个已配置 SSID 执行 hints::load_hints(ssid)
            过期文件 -> 删除，不加载提示
        build_shuli_network(cfg, hints)
            network.set_hints(frequencies_mhz + known_bss)
    shuli：
        首次扫描限制在提示信道
        可见的已知 BSS 中信号最强者胜出（尊重 prefered）
        没有可见的已知 BSS -> 重试一次全扫描，再走正常选择
        连接到支持漫游的 BSS 后 -> 一次性更优 BSS 扫描，
        发现严格更强时切换
```

代码索引：

| 部件 | 位置 |
|------|------|
| 提示文件处理 | `src/plugin-wifi/hints.rs` |
| 连接成功时记录 | `WifiClientState::run_once()`，`src/plugin-wifi/apply.rs`（`save_quick_start_hints()`） |
| 加载与附加 | `WifiClientState::apply()`、`build_shuli_network()`（`src/plugin-wifi/apply.rs`） |
| 比较时忽略提示 | `same_saved_networks_ignoring_prefered()`、`refresh_hints()`（`src/plugin-wifi/apply.rs`） |
| 配置删除时清理 | 构建 `ssids_to_delete` 处的 `hints::remove()` 调用（`src/plugin-wifi/apply.rs`） |
| 已知 BSS 提示类型 | `KnownBss`、`NetworkConfigHints::known_bss`（shuli `src/lib/config.rs`） |
| 已提示 BSS 选择 | `best_hinted_candidate_filtered()`、`process_scan_results()`（shuli `src/lib/scan.rs`） |
| 连接后最优 BSS 扫描 | `arm_post_connect_scan()`、`post_connect_best_bss_scan()`、`connected_wait_duration()`（shuli `src/lib/roam.rs`）、已连接等待循环（shuli `src/lib/client/wifi_iface.rs`） |

## 提示文件

### 位置与文件名

`/var/lib/nipart/wifi/<编码后的 SSID>.hints`。

目录在首次写入时以 `0700` 权限惰性创建。SSID 最长 32 个 UTF-8 字节，
且不是安全的路径组成部分：`/`、空格和开头的 `.` 都是合法字符。所有
不在 `[A-Za-z0-9._-]` 中的字节按 UTF-8 字节进行百分号编码，例如
`My Wifi/5G` -> `My%20Wifi%2F5G.hints`。SSID 同时保存在文件内容中，
因此被改名或伪造的文件在加载时会被丢弃。

文件只以 SSID 为键，不区分 phy：同一 SSID 在多个 wifi-phy 上连接时
共用一个文件；在同一 ESS 的多个 AP 之间漫游会向文件中累积多个 BSS
条目。

### 文件格式

带版本号的 YAML（`rmsd_yaml`，与 `applied.yml` 约定一致）：

```yaml
version: 1
ssid: My Wifi/5G
updated_on: 20628        # UTC 日序号（自 1970-01-01 起的天数）
bss:
- bssid: 02:00:00:00:00:01
  frequency_mhz: 5180
- bssid: 02:00:00:00:00:02
  frequency_mhz: 2412
```

* `updated_on` 有意只精确到天：同一天以相同 BSS 集合重连不会写入，
  因此稳定的单 AP 环境每天最多写入一次。
* `bss` 最多保存 `MAX_BSS_PER_SSID`（16）条；超出上限时丢弃最旧的条目。
* 文件中绝不含凭据、RSNE/RSNXE、加密套件或信号数据 - 只保存快速扫描
  所需的信息。

### 校验

`hints::read_hints()` 在以下情况丢弃文件（写 debug 日志，返回空提示）：

* 路径不是普通文件（拒绝符号链接；文件以 `O_NOFOLLOW` 打开）；
* 文件大于 64 KiB；
* 文件对组或其他用户可写；
* 文件属主与其目录属主不一致（生产环境中目录属主为 root）；
* 无法按预期 YAML 解析；
* `version` 不是支持的版本，或内嵌 `ssid` 与请求的 SSID 不一致。

### 写入路径

`WifiClientState::run_once()` 在每次上报 `Connected*` 状态（首次连接、
重连、成功漫游）时调用 `hints::record_bss(ssid, bssid, freq_mhz)`，
数据来自 `WifiClient::export_hints()`（`shuli`）。它会：

1. 读取已有文件（若已过期则视为不存在）；
2. 按 BSSID 更新或追加条目（频率变化时更新该条目）；
3. 执行条目上限裁剪并刷新 `updated_on`；
4. 仅在内容变化或日期变化时写入；否则跳过 - 这也是"同一 BSS、
   同一天"稳态下的无操作。

错误只记录日志并被吞掉：绝不会影响连接状态。

### 读取路径

`WifiClientState::apply()` 对每个已配置 SSID 调用一次
`hints::load_frequencies(ssid)`，返回已排序、去重的频率并集。过期文件
在这里被删除 - 选择在 SSID 被应用时清理过期提示。

`build_shuli_network()` 将该列表附加为
`NetworkConfigHints { frequencies_mhz, ..default }`；没有提示的 SSID
不会设置任何 hint 对象。

### 生命周期与清理

* 每次成功连接新的 BSS（新 AP、漫游）时记录。
* 当 `wifi-cfg` 配置被显式删除（`absent`/`down`）时，与其内存中的
  SSID 记录一起删除。
* `wifi off`、phy absent/down、守护进程重启和回滚时保留：这些操作
  并未删除已保存的配置。
* 加载时发现文件早于 `HINT_TTL_DAYS`（30 天）则删除。
* `tests/run-tests.sh` 会在测试前后快照并恢复
  `/var/lib/nipart/wifi`（与现有的 `/etc/nipart` 处理一致），避免
  残留提示在不同测试之间泄漏。

### 原子性

文件先写入同目录下的 `.{name}.hints.tmp.<pid>`（权限 `0600`），再用
`rename()` 覆盖旧文件，因此读取方不会看到残缺文件。不执行 `fsync`：
与 `applied.yml` 的约定保持一致，崩溃时可能丢失提示，但下一次连接会
重新记录。

## 将提示附加到 shuli

nipart 把保存的频率与 BSSID 作为
`NetworkConfigHints { frequencies_mhz, known_bss, .. }` 附加到 shuli 的
`NetworkConfig`。完整的单 BSS 无扫描字段（BSSID、安全类型、RSNE）保持
未设置，因此 shuli 的无扫描直接认证路径保持关闭；提示用于驱动快速扫描与
已提示 BSS 选择。

`wifi-cfg.bssid` 与上述提示不同：设置后，插件将其传给
`NetworkConfig::set_bssid()` 作为硬性固定，shuli 只会选择该 AP，绝不
回退到同一 SSID 的其他 BSS（扫描选择、重试与漫游都遵守该约束）。非法
的 MAC 会使应用以 `InvalidArgument` 失败，而不是静默连接到任意 BSS。

### 为何不使用无扫描提示

只有当某个已配置网络的提示是完整的（`NetworkConfigHints::bss_info()`
能返回 `BssInfo`）时，shuli 才会启用无扫描直接认证路径。快速启动提示
有意保持不完整，因此 shuli 走提示扫描路径。选择该策略而非无扫描认证的
原因有两点：

* **时延安全。** 无扫描认证直接向保存的 BSSID 发送
  `NL80211_CMD_AUTHENTICATE`，然后最多等待
  `AUTH_EVENT_TIMEOUT_SECS`（15 秒）的认证事件。冷启动时该 BSSID 不在
  cfg80211 缓存中，命令会立刻以 `-ENOENT` 失败；但当内核仍缓存着过期
  BSS 时（守护进程重启、AP 已关机），客户端会等满 15 秒才回退。一次
  快速信道扫描的耗时远小于此，并且能为选择提供实时信号数据，因此频率
  提示既更安全，也对多 BSS 的 ESS 更有用。
* **多 BSS 排序。** 无扫描尝试只能针对一个 BSS，且 shuli 会选择第一个
  带有完整提示的网络。提示扫描能看到这些信道上的所有 BSS，并在已知
  BSS 中按实时信号选择最优者。

### 已提示 BSS 选择

带有 `known_bss` 条目时，提示快速扫描的工作方式如下：

* 每个已配置网络都贡献自己的提示：扫描频率并集
  （`configured_hint_frequencies()`）同时纳入各网络的
  `frequencies_mhz` 与 `known_bss[].frequency_mhz`；选择过程考虑所有
  网络的 `known_bss` 列表，而不只是第一个；
* 首次主机扫描被限制在提示信道上；
* `process_scan_results()` 构建已提示候选集（BSSID 属于某个已配置网络
  `known_bss` 的扫描结果），并在现有 `prefered` 层级规则内连接到其中
  信号最强者。提示信道上出现的非提示 BSS 绝不会被静默优先于可见的
  已知 BSS；
* 没有可见的已知 BSS 时，shuli 重试一次全扫描并回退到正常选择，从而
  仍能发现换信道的 AP 或同一 SSID 的新 BSS。

`WifiClient::export_hints()` 把当前链路输出为单条目 `known_bss` 列表，
API 使用者可以按 nipart 记录提示文件的同样方式累积该列表。

### 连接后最优 BSS 扫描

在全新连接建立后，如果网络启用了漫游且所连 BSS 宣告受管漫游支持
（802.11v BTM 或 802.11k 邻居报告），shuli 会安排一次性最优 BSS 扫描。
扫描在链路建立 `POST_CONNECT_SCAN_DELAY_SECS`（5 秒）后运行，避免与
DHCP 交换和首批流量竞争；信号触发或后台漫游扫描会取代尚未执行的该
扫描。由于连接扫描已覆盖提示信道，这里使用全扫描；仅在同一 ESS 中存在
严格更强的 BSS 时切换，只有当前链路处于临界状态时才考虑切换到其他已
配置 SSID。与信号触发路径不同，当前信号健康不会取消该扫描 - 切换到
严格更强的 BSS 正是它的目的。

一个 nipart 侧细节：`NetworkConfig` 的相等比较包含 `hints`，因此
`same_saved_networks_ignoring_prefered()` 在比较（已保存的）网络列表前
会清空 `hints`。否则连接期间记录的 BSS 会被误判为配置变化，在下一次
幂等重复应用时重启正在运行的 shuli 客户端。当仅提示不同时，
`refresh_hints()` 会更新供下次客户端启动使用的存储列表，而不触碰运行
中的客户端。

## 安全性

* 提示文件不含凭据：没有 password/PSK/PMK，甚至没有 RSNE/RSNXE/加密
  套件数据。WIFI 密码仍保存在 `/etc/nipart/applied.secrets.yml`。
* 目录权限 `0700`，文件权限 `0600`；拒绝符号链接，文件属主必须与
  目录属主一致，忽略对组/其他用户可写的文件，解析前有 64 KiB 大小
  上限。
* 提示内容是不可信输入，但它只能收窄或误导首次扫描。shuli 仍会从实时
  扫描获知 AP 的真实安全类型并据此认证；提示无法授予访问权限。
* 能写入 `/var/lib/nipart` 的本地攻击者已等价于 root，因此不宣称
  额外的权限边界。

## 故障处理

| 情况 | 行为 |
|------|------|
| 提示文件缺失 | 正常全信道扫描。 |
| 提示文件损坏 / 版本错误 / SSID 不匹配 / 过大 / 属主或权限错误 / 符号链接 | 写 debug 日志后忽略；正常全信道扫描。 |
| 提示指向的信道没有发现任何已配置 SSID（AP 换信道、已关机） | shuli 重试一次全扫描。 |
| 提示写入失败（磁盘满、`/var/lib` 只读） | 写 warn 日志；连接不受影响。 |
| 文件早于 30 天 | 在其 SSID 被应用时忽略并删除。 |
| `wifi-cfg` 配置被删除 | 提示文件随配置一起删除。 |

## 与接口监视器和启动激活的交互

无需修改守护进程协议。启动时监视器/事件工作线程会在 phy 出现时把已
保存的 `wifi-cfg` 配置交给 wifi 插件，该路径与用户手动应用一样进入
`WifiClientState::apply()`，提示即在此加载。回滚路径
（`restore_saved_wifi_profiles()`）也通过同一代码重建插件的网络列表。

## 测试

只做单元测试，这是有意为之：

* `src/plugin-wifi/unit_tests/hints.rs` 覆盖 SSID 编码/路径安全、
  记录与加载往返、同日不写入规则、按天的 TTL 边界、BSS 上限、更新/
  频率变化、损坏 / 版本错误 / SSID 不匹配 / 过大 / 符号链接 / 可写
  文件、删除的幂等性、文件权限，以及序列化 YAML 中不含任何密钥或
  interface 字段。
* `src/plugin-wifi/unit_tests/apply.rs` 覆盖提示（频率与已知 BSS）只
  附加到匹配的 SSID、变化检测/刷新行为，以及 `wifi-cfg.bssid` 固定：
  映射到 `NetworkConfig::set_bssid()`、非法 MAC 拒绝与固定冲突检测。
* `shuli` crate 覆盖 `KnownBss` 提示往返与"仅已知 BSS 不禁用无扫描路径"
  的规则、已提示 BSS 选择（`best_hinted_candidate_filtered`：已知 BSS
  优先于更强的未知 BSS、最强已知 BSS、`prefered` 层级、无可见已知
  BSS）以及连接后扫描的等待计算（`connected_wait_duration`），并在其
  既有 wifi 集成测试套件之上运行。

刻意跳过"快速启动确实发生"的集成测试：在共享测试环境中，持续负载或
繁忙信道可能让首次扫描比全扫描还慢，因此该行为/时序无法可靠观测，
这样的测试只会是不稳定的，而非有意义的。扫描提示路径本身已由 shuli
的测试覆盖。

## 后续工作

* **无扫描提示**：若日后需要直接认证，应使用较短的提示认证超时
  （从 1 秒起步），让过期的 BSS 快速回退。一次开发环境实验
  （mac80211_hwsim、WPA2-PSK 与 WPA3-SAE）在 1 秒认证事件超时下约
  130 毫秒完成连接；真实射频环境验证尚未进行。本设计不需要它。
* **无 802.11v/11k 的 AP 上的主动漫游**：shuli 仅在 AP 宣告 BTM
  （802.11v）或邻居报告（802.11k）时才执行信号触发的漫游扫描；其他
  AP 只在断开后才会被重新评估。不在本文范围内。
