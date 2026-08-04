<!-- vim-markdown-toc GFM -->

* [路由](#路由)
    * [`running`：当前内核路由](#running当前内核路由)
    * [`config`：期望的静态路由](#config期望的静态路由)
    * [`state`：路由状态](#state路由状态)
    * [`destination`：路由目标网络](#destination路由目标网络)
    * [`next-hop-interface`：下一跳接口](#next-hop-interface下一跳接口)
    * [`next-hop-address`：下一跳 IP 地址](#next-hop-address下一跳-ip-地址)
    * [`metric`：路由度量值](#metric路由度量值)
    * [`table-id`：路由表 ID](#table-id路由表-id)
    * [`weight`：ECMP 路由权重](#weightecmp-路由权重)
    * [`route-type`：路由类型](#route-type路由类型)
    * [`source`：源地址](#source源地址)
    * [`cwnd`：拥塞窗口限制](#cwnd拥塞窗口限制)
    * [`initcwnd`：初始拥塞窗口](#initcwnd初始拥塞窗口)
    * [`initrwnd`：初始接收窗口](#initrwnd初始接收窗口)
    * [`mtu`：路由 MTU](#mtu路由-mtu)
    * [`quickack`：快速 ACK](#quickack快速-ack)
    * [`advmss`：通告 MSS](#advmss通告-mss)

<!-- vim-markdown-toc -->
# 路由

路由配置的 YAML 示例：

```yaml
version: 1
routes:
  config:
  - destination: 0.0.0.0/0
    next-hop-interface: eth1
    next-hop-address: 192.0.2.1
    metric: 100
    table-id: 254
  - destination: 0.0.0.0/0
    next-hop-interface: eth1
    next-hop-address: 192.0.2.2
    metric: 100
    weight: 2
  - destination: 2001:db8::/64
    next-hop-interface: eth1
    next-hop-address: 2001:db8::1
  - destination: 10.0.0.0/24
    next-hop-interface: eth1
    next-hop-address: 192.0.2.254
  - state: absent
    next-hop-interface: eth1
```

## `running`：当前内核路由

仅查询属性。包含来自内核的当前活跃路由，筛选范围为宇宙（universe）或链路
（link）作用域，且仅来自以下协议：`boot`、`static`、`ra`、`dhcp`、
`mrouted`、`keepalived`、`babel`。

应用时忽略。

## `config`：期望的静态路由

期望的静态路由。包含宇宙或链路作用域的路由，仅来自协议 `boot` 和 `static`。

应用时，`None` 表示保留当前路由。此属性不是覆盖而是将指定的路由添加到现有
路由中。要删除某个路由条目，请将 `state` 设置为 `absent`。对于状态为 absent
的 `RouteEntry`，任何设置为 `None` 的属性都表示通配符匹配。例如，以下配置将
删除所有下一跳为接口 `eth1` 的路由：

```yaml
routes:
  config:
  - next-hop-interface: eth1
    state: absent
```

要更改某个路由条目，你需要删除旧条目并添加新条目（可以在单个事务中完成）。

## `state`：路由状态

仅用于应用时删除路由：
 * `absent`：标记要删除的路由条目。设置为 `None` 的属性充当匹配要删除路由的
   通配符。
 * `ignore`：将路由标记为不受 nipart 管理。

## `destination`：路由目标网络

路由的目标网络，以 CIDR 表示法表示，例如 `0.0.0.0/0` 表示默认网关，
`10.0.0.0/24` 表示子网。

每个非 absent 路由必填。

`0.0.0.0/8` 及其子网不能用作单播路由的路由目标。请改用 `0.0.0.0/0`
表示默认网关。

## `next-hop-interface`：下一跳接口

下一跳的接口名称，例如 `eth1`。

每个非 absent 单播路由必填。路由类型为 `Blackhole`、`Unreachable` 或
`Prohibit` 的路由不需要。

## `next-hop-address`：下一跳 IP 地址

下一跳路由器的 IP 地址，例如 `192.0.2.1`。

可选。对于 absent 路由设置为空字符串时，仅删除没有 `next-hop-address` 的路由。

## `metric`：路由度量值

路由度量值（优先级）。默认值由后端定义。度量值越低越优先。

## `table-id`：路由表 ID

路由表 ID。默认为 `254`（主路由表）。设置为 `0` 以使用后端默认值。

## `weight`：ECMP 路由权重

等价多路径（ECMP）路由的权重。有效范围是 1 到 256。

当多个路由条目共享相同的 `destination` 和 `metric`，但具有不同的
`next-hop-address` 时，它们构成 ECMP 路由。内核根据权重按比例分配流量。

尚不支持带权重的 IPv6 ECMP 路由。

## `route-type`：路由类型

路由类型：
 * `blackhole`：匹配此路由的数据包被静默丢弃。
 * `unreachable`：匹配此路由的数据包生成 ICMP 不可达消息。
 * `prohibit`：匹配此路由的数据包生成 ICMP 管理性禁止消息。

没有 `route-type` 的路由是单播路由（默认）。

非单播路由不能有 `next-hop-interface`（`lo` 除外）或 `next-hop-address`。

## `source`：源地址

通过此路由发送的数据包的首选源地址。指定匹配此路由的出站数据包应使用哪个
本地 IP 地址作为源地址。

## `cwnd`：拥塞窗口限制

拥塞窗口限制大小（字节）。不能设置为 0。

## `initcwnd`：初始拥塞窗口

初始拥塞窗口大小（字节）（TCP initcwnd）。

## `initrwnd`：初始接收窗口

初始接收窗口大小（字节）（TCP initrwnd）。

## `mtu`：路由 MTU

路由的 MTU（字节）。不能设置为 0。

## `quickack`：快速 ACK

当设置为 `true` 时，禁用使用此路由的连接的延迟 TCP 确认。

## `advmss`：通告 MSS

使用此路由的 TCP 连接要通告的最大分段大小（MSS）。不能设置为 0。
