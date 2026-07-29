<!-- vim-markdown-toc GFM -->

* [Wireguard](#wireguard)
    * [`private-key`：私钥](#private-key私钥)
    * [`public-key`：公钥](#public-key公钥)
    * [`listen-port`：监听端口](#listen-port监听端口)
    * [`fwmark`：防火墙标记](#fwmark防火墙标记)
    * [`peers`：对等方配置](#peers对等方配置)
        * [`endpoint`：对等方端点](#endpoint对等方端点)
        * [`public-key`：对等方公钥](#public-key对等方公钥)
        * [`preshared-key`：预共享密钥](#preshared-key预共享密钥)
        * [`persistent-keepalive`：持久保活](#persistent-keepalive持久保活)
        * [`allowed-ips`：允许的 IP](#allowed-ips允许的-ip)
        * [`protocol-version`：协议版本](#protocol-version协议版本)
        * [`last-handshake`：上次握手时间](#last-handshake上次握手时间)
        * [`rx-bytes`：接收字节数](#rx-bytes接收字节数)
        * [`tx-bytes`：发送字节数](#tx-bytes发送字节数)

<!-- vim-markdown-toc -->
# Wireguard

WireGuard 接口配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: wg0
  type: wireguard
  state: up
  wireguard:
    private-key: xH4dTz3dN3LzP2gE2kR8pA7sV9cF0bN1mQ5wY6uJ8k=
    listen-port: 51820
    fwmark: 0
    peers:
    - endpoint: 192.0.2.1:51820
      public-key: r3V5cF0bN1mQ5wY6uJ8k=xH4dTz3dN3LzP2gE2kR8pA7sV9=
      preshared-key: p7sV9cF0bN1mQ5wY6uJ8k=xH4dTz3dN3LzP2gE2kR8pA=
      persistent-keepalive: 25
      allowed-ips:
      - ip: 10.0.0.0
        prefix-length: 24
      - ip: 192.168.0.0
        prefix-length: 16
```

## `private-key`：私钥

Base64 编码的私钥。创建新 WireGuard 接口时必填。在调试/显示输出中将显示为
`<_hidden_>`。

对现有接口应用时设置为 `<_hidden_>` 可保持当前私钥不变。

## `public-key`：公钥

Base64 编码的公钥。仅查询属性，应用时忽略。

## `listen-port`：监听端口

用于监听传入连接的 UDP 端口。如果未定义，内核将选择一个随机端口。

## `fwmark`：防火墙标记

出站数据包的防火墙标记（fwmark）值。

## `peers`：对等方配置

对等方配置列表。如果定义了，将覆盖现有的对等方列表。如果未定义，将保留
当前的对等方。

### `endpoint`：对等方端点

对等方的端点地址和端口，格式为 `ip:port`，例如 `192.0.2.1:51820`。
每个对等方配置必填。

### `public-key`：对等方公钥

对等方的 Base64 编码公钥。用于识别对等方。

### `preshared-key`：预共享密钥

Base64 编码的预共享密钥，通过对称密钥加密提供额外的安全性。
在调试/显示输出中显示为 `<_hidden_>`。

对现有接口应用时设置为 `<_hidden_>` 可保持当前预共享密钥不变。

### `persistent-keepalive`：持久保活

保活数据包之间的间隔（秒）。用于维护 NAT/网桥映射。

### `allowed-ips`：允许的 IP

此对等方允许的 IP 前缀列表。每个条目包含：
 * `ip`：IP 地址。
 * `prefix-length`：前缀长度（CIDR 掩码）。

### `protocol-version`：协议版本

WireGuard 协议版本。

### `last-handshake`：上次握手时间

仅查询属性。显示自上次握手以来的时间（例如 `32 seconds ago`）。

### `rx-bytes`：接收字节数

仅查询属性。从此对等方接收的总字节数。

### `tx-bytes`：发送字节数

仅查询属性。向此对等方发送的总字节数。
