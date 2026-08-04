<!-- vim-markdown-toc GFM -->

* [IP 地址](#ip-地址)
    * [`enabled`：启用 IP 协议栈](#enabled启用-ip-协议栈)
    * [`dhcp`：启用 DHCP](#dhcp启用-dhcp)
    * [`address`：IP 地址配置](#addressip-地址配置)
    * [`autoconf`：启用 IPv6 自动配置](#autoconf启用-ipv6-自动配置)

<!-- vim-markdown-toc -->

# IP 地址

静态 IP 地址配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: eth1
  type: veth
  state: up
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: 192.0.2.252
      prefix-length: 24
    - ip: 192.0.2.251
      prefix-length: 24
  ipv6:
    enabled: true
    dhcp: false
    autoconf: false
    address:
    - ip: 2001:db8:1::1
      prefix-length: 64
    - ip: 2001:db8:2::1
      prefix-length: 64
```


## `enabled`：启用 IP 协议栈

当设置为 `false` 时，该接口的 IP 协议栈将被禁用。这意味着该接口将无法发送或接收 IP 数据包。

## `dhcp`：启用 DHCP

当设置为 `true` 时，该接口将尝试通过 DHCP 获取 IP 地址。仅在 `enabled` 设置为 `true` 时有效。

**注意**：DHCPv6 无法设置 IPv6 路由。请使用 `autoconf` 通过 IPv6 路由通告来获取 IPv6 地址和路由。

## `address`：IP 地址配置

`address` 字段是该接口的 IP 地址配置列表。

每个条目应包含以下字段：
- `ip`：要分配给接口的 IP 地址。
- `prefix-length`：IP 地址的前缀长度（子网掩码）。
- `valid-life-time`：（可选）IP 地址的有效生存时间（秒）。如果未指定，该 IP 地址将被视为静态 IP，永久有效。
- `preferred-life-time`：（可选）IP 地址的首选生存时间（秒）。如果未指定，该 IP 地址将被视为静态 IP，永久首选。

## `autoconf`：启用 IPv6 自动配置

当设置为 `true` 时，该接口将尝试通过 IPv6 路由通告获取 IPv6 地址。仅在 `enabled` 设置为 `true` 时有效。
