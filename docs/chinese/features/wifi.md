<!-- vim-markdown-toc GFM -->

* [WIFI](#wifi)
    * [`ssid`：SSID](#ssidssid)
    * [`state`：Wifi 状态](#statewifi-状态)
    * [`bssid`：BSSID](#bssidbssid)
    * [`password`：密码](#password密码)
    * [`base-iface`：基础接口](#base-iface基础接口)
    * [`auth-type`：认证类型](#auth-type认证类型)
    * [`generation`：Wifi 代际](#generationwifi-代际)
    * [`frequency-mhz`：频率](#frequency-mhz频率)
    * [`rx-bitrate-mb`：接收比特率](#rx-bitrate-mb接收比特率)
    * [`tx-bitrate-mb`：发送比特率](#tx-bitrate-mb发送比特率)
    * [`signal-dbm`：信号强度](#signal-dbm信号强度)
    * [`signal-percent`：信号百分比](#signal-percent信号百分比)

<!-- vim-markdown-toc -->
# WIFI

Nipart 中的 WIFI 配置使用两种接口类型：

- **`wifi-phy`**：内核 WiFi 物理接口（例如 `wlan0`）。用于当前状态（查询结果），
  或在物理接口名称事先已知的情况下使用。
- **`wifi-cfg`**：一个虚拟/仅用户空间的接口，保存期望的 WiFi 连接配置。
  它没有内核索引，仅存在于 Nipart 的期望状态中。你可以通过 `base-iface`
  有选择地将其绑定到特定的 `wifi-phy`，或不绑定（表示"任意可用的 wifi-phy"）。

使用静态 IP 的 WIFI 配置 YAML 示例：

```yaml
version: 1
routes:
  config:
  - destination: 0.0.0.0/0
    next-hop-interface: wlan0
    next-hop-address: 192.0.2.1
    metric: 100
interfaces:
- name: wlan0
  type: wifi-phy
  state: up
  mtu: 1492
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: 192.0.2.6
      prefix-length: 24
  wifi:
    ssid: SweatHome5G
    bssid: D0:21:F9:49:B3:52
    password: <_hidden_>
```

## `ssid`：SSID

要连接的 WiFi 网络的 SSID（服务集标识符）。

## `state`：Wifi 状态

仅查询属性。WiFi 链路的当前连接状态：

- `disconnected`：BSS 已断开连接
- `scanning`：正在扫描 SSID
- `connecting`：已找到 SSID，正在尝试与 BSS/SSID 关联和认证
- `completed`：数据连接已完全配置并正常运行
- `unknown`：无法确定状态

## `bssid`：BSSID

接入点的 BSSID（基本服务集标识符）。设置后，Nipart 将仅连接到指定的 AP。
如果省略，可以使用广播该 SSID 的任意 AP。

## `password`：密码

用于认证的密码或预共享密钥。查询当前状态时，此字段会被替换为 `<_hidden_>`。

## `base-iface`：基础接口

将此配置绑定到的 WiFi 物理接口的内核名称。如果设置，WiFi 连接将被限制在该
特定接口上。

使用 `wifi-phy` 类型时，此字段默认为接口名称本身。
使用 `wifi-cfg` 类型并设置 `base-iface: <name>` 时，配置将绑定到该物理接口。
未定义（未绑定）时，配置适用于任何符合条件的 `wifi-phy` 接口。

## `auth-type`：认证类型

仅查询属性。当前连接的简化认证类型，供用户应用和查询。应用时忽略。

支持的认证类型：

- `OPEN`：无认证（开放网络）
- `WPA2-PSK`：WPA 2 预共享密钥
- `WPA3-PSK`：使用 SAE 的 WPA 3 预共享密钥
- `unknown`：无法确定

`npt wifi scan` 的每个 `WifiScanResult` 条目使用 `auth-types` 报告详细认证
类型列表。每个条目包含简化的 `auth-type`，以及接入点通告的 AKM（认证与
密钥管理）套件和密码套件，例如：

```yaml
- ssid: SweatHome5G
  base-iface: wlan0
  bssid: d0:21:f9:49:b3:52
  frequency-mhz: 5180
  signal-dbm: -45
  signal-percent: 78
  auth-types:
  - auth-type: WPA2-PSK
    akm:
    - PSK
    cipher:
    - CCMP
```

## `generation`：Wifi 代际

仅查询属性。WiFi 代际，例如 `6` 表示 WiFi 6。

## `frequency-mhz`：频率

仅查询属性。WiFi 频率（MHz）。

## `rx-bitrate-mb`：接收比特率

仅查询属性。接收比特率（1 Mb/s 为单位）。

## `tx-bitrate-mb`：发送比特率

仅查询属性。发送比特率（1 Mb/s 为单位）。

## `signal-dbm`：信号强度

仅查询属性。信号强度（dBm）。

## `signal-percent`：信号百分比

仅查询属性。信号强度百分比（0-100）。
