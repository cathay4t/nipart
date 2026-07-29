<!-- vim-markdown-toc GFM -->

* [VxLAN](#vxlan)
    * [`base-iface`：基础接口](#base-iface基础接口)
    * [`id`：VxLAN ID (VNI)](#idvxlan-id-vni)
    * [`remote`：远程隧道端点](#remote远程隧道端点)
    * [`local`：本地隧道端点](#local本地隧道端点)
    * [`destination-port`：目标端口](#destination-port目标端口)
    * [`learning`：FDB 学习](#learningfdb-学习)
    * [`ttl`：IP TTL](#ttlip-ttl)
    * [`tos`：IP TOS](#tosip-tos)
    * [`ageing`：FDB 条目的老化时间](#ageingfdb-条目的老化时间)
    * [`max-address`：最大 FDB 条目数](#max-address最大-fdb-条目数)
    * [`src-port-min` / `src-port-max`：源端口范围](#src-port-min--src-port-max源端口范围)
    * [`proxy`：ARP 代理](#proxyarp-代理)
    * [`rsc`：路由短路](#rsc路由短路)
    * [`l2miss`：L2 未命中通知](#l2missl2-未命中通知)
    * [`l3miss`：L3 未命中通知](#l3missl3-未命中通知)
    * [`udp-check-sum`：UDP 校验和](#udp-check-sumudp-校验和)
    * [`udp6-zero-check-sum-tx`：IPv6 UDP 零校验和发送](#udp6-zero-check-sum-txipv6-udp-零校验和发送)
    * [`udp6-zero-check-sum-rx`：IPv6 UDP 零校验和接收](#udp6-zero-check-sum-rxipv6-udp-零校验和接收)
    * [`remote-check-sum-tx`：远程校验和发送](#remote-check-sum-tx远程校验和发送)
    * [`remote-check-sum-rx`：远程校验和接收](#remote-check-sum-rx远程校验和接收)
    * [`gbp`：基于组的策略](#gbp基于组的策略)
    * [`remote-check-sum-no-partial`：远程校验和无部分](#remote-check-sum-no-partial远程校验和无部分)
    * [`collect-metadata`：收集元数据](#collect-metadata收集元数据)
    * [`label`：流标签](#label流标签)
    * [`gpe`：通用协议扩展](#gpe通用协议扩展)
    * [`ttl-inherit`：TTL 继承](#ttl-inheritttl-继承)

<!-- vim-markdown-toc -->

# VxLAN

> **注意：** 以下属性可以在活跃接口上实时更改，无需删除：
>  * `remote`
>  * `local`
>  * `learning`
>  * `ttl`
>  * `tos`
>  * `ageing`
>  * `label`
>
> 所有其他 VxLAN 属性的更改将触发接口删除和重建。

VxLAN 接口配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: vxlan100
  type: vxlan
  state: up
  ipv4:
    enabled: true
    dhcp: true
  vxlan:
    base-iface: eth1
    id: 100
    remote: 192.0.2.251
    local: 192.0.2.252
    learning: true
    destination-port: 4789
    ttl: 0
    tos: 0
```

## `base-iface`：基础接口

创建 VxLAN 隧道所使用的物理或父接口名称，例如 `eth1`。VxLAN 将在此接口上
封装流量。

创建新 VxLAN 接口时必填。对现有 VxLAN 应用更改时，留空将保留当前的基础接口。

## `id`：VxLAN ID (VNI)

VxLAN 网络标识符（VNI）。有效范围是 0 到 16777215（24 位）。

创建新 VxLAN 接口时必填。对现有 VxLAN 应用更改时，留空将保留当前的 ID。

## `remote`：远程隧道端点

远程 VXLAN 隧道端点（VTEP）的单播或组播 IP 地址，例如 `192.0.2.251` 或
`2001:db8::1`。

## `local`：本地隧道端点

本地 VXLAN 隧道端点（VTEP）的 IP 地址，例如 `192.0.2.252` 或 `2001:db8::2`。

## `destination-port`：目标端口

VxLAN 通信的 UDP 目标端口。如果未定义，默认为 `4789`（IANA 分配的 VxLAN
端口）。

## `learning`：FDB 学习

当设置为 `true` 时，网桥的 VXLAN 学习功能被启用，允许内核自动填充 FDB。
如果未定义，默认为 `true`。

## `ttl`：IP TTL

用于 VxLAN 隧道协议 IP 头部的 TTL 值。

## `tos`：IP TOS

用于 VxLAN 隧道协议 IP 头部的 TOS（服务类型）值。

## `ageing`：FDB 条目的老化时间

内核学习的 FDB 条目的生存时间（秒）。

## `max-address`：最大 FDB 条目数

此 VxLAN 接口允许的最大 FDB 条目数。

## `src-port-min` / `src-port-max`：源端口范围

用于 VxLAN 通信的 UDP 源端口范围。必须同时指定 `src-port-min` 和
`src-port-max` 来定义该范围。

## `proxy`：ARP 代理

当设置为 `true` 时，在 VxLAN 接口上启用 ARP 代理。

## `rsc`：路由短路

当设置为 `true` 时，启用路由短路。

## `l2miss`：L2 未命中通知

当设置为 `true` 时，FDB 中的 L2 地址查找未命中时会生成 netlink 通知。

## `l3miss`：L3 未命中通知

当设置为 `true` 时，FDB 中的 L3 地址查找未命中时会生成 netlink 通知。

## `udp-check-sum`：UDP 校验和

当设置为 `true` 时，为 VxLAN 隧道启用 UDP 校验和计算。

## `udp6-zero-check-sum-tx`：IPv6 UDP 零校验和发送

当设置为 `true` 时，允许 IPv6 隧道发送零校验和 UDP 数据包。

## `udp6-zero-check-sum-rx`：IPv6 UDP 零校验和接收

当设置为 `true` 时，允许 IPv6 隧道接收零校验和 UDP 数据包。

## `remote-check-sum-tx`：远程校验和发送

当设置为 `true` 时，启用发送的远程校验和卸载。

## `remote-check-sum-rx`：远程校验和接收

当设置为 `true` 时，启用接收的远程校验和卸载。

## `gbp`：基于组的策略

当设置为 `true` 时，启用基于组的策略扩展。

## `remote-check-sum-no-partial`：远程校验和无部分

当设置为 `true` 时，禁用部分远程校验和。

## `collect-metadata`：收集元数据

当设置为 `true` 时，VxLAN 接口从入口数据包中收集元数据。

## `label`：流标签

VxLAN 隧道的 IPv6 流标签。仅适用于 IPv6。

## `gpe`：通用协议扩展

当设置为 `true` 时，启用通用协议扩展（GPE），允许承载除以太网之外的其他
协议。

## `ttl-inherit`：TTL 继承

当设置为 `true` 时，VxLAN 隧道继承内部数据包的 TTL。
