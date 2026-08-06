<!-- vim-markdown-toc GFM -->

* [Linux 网桥](#linux-网桥)
    * [`options`：网桥选项](#options网桥选项)
        * [`group-addr`：组播组地址](#group-addr组播组地址)
        * [`group-fwd-mask`：组转发掩码](#group-fwd-mask组转发掩码)
        * [`hash-max`：哈希表最大值](#hash-max哈希表最大值)
        * [`mac-ageing-time`：MAC 老化时间](#mac-ageing-timemac-老化时间)
        * [`multicast-last-member-count`：最后成员查询次数](#multicast-last-member-count最后成员查询次数)
        * [`multicast-last-member-interval`：最后成员查询间隔](#multicast-last-member-interval最后成员查询间隔)
        * [`multicast-membership-interval`：成员资格间隔](#multicast-membership-interval成员资格间隔)
        * [`multicast-querier`：组播查询器](#multicast-querier组播查询器)
        * [`multicast-querier-interval`：查询器间隔](#multicast-querier-interval查询器间隔)
        * [`multicast-query-interval`：查询间隔](#multicast-query-interval查询间隔)
        * [`multicast-query-response-interval`：查询响应间隔](#multicast-query-response-interval查询响应间隔)
        * [`multicast-query-use-ifaddr`：查询使用接口地址](#multicast-query-use-ifaddr查询使用接口地址)
        * [`multicast-router`：组播路由器类型](#multicast-router组播路由器类型)
        * [`multicast-snooping`：组播监听](#multicast-snooping组播监听)
        * [`multicast-startup-query-count`：启动查询次数](#multicast-startup-query-count启动查询次数)
        * [`multicast-startup-query-interval`：启动查询间隔](#multicast-startup-query-interval启动查询间隔)
        * [`stp`：生成树协议](#stp生成树协议)
            * [`enabled`：STP 启用](#enabledstp-启用)
            * [`forward-delay`：转发延迟](#forward-delay转发延迟)
            * [`hello-time`：Hello 时间](#hello-timehello-时间)
            * [`max-age`：最大老化时间](#max-age最大老化时间)
            * [`priority`：网桥优先级](#priority网桥优先级)
        * [`vlan-protocol`：VLAN 协议](#vlan-protocolvlan-协议)
        * [`vlan-default-pvid`：默认 PVID](#vlan-default-pvid默认-pvid)
    * [`ports`：网桥端口](#ports网桥端口)
        * [`name`：端口名称](#name端口名称)
        * [`stp-hairpin-mode`：Hairpin 模式](#stp-hairpin-modehairpin-模式)
        * [`stp-path-cost`：STP 路径成本](#stp-path-coststp-路径成本)
        * [`stp-priority`：STP 优先级](#stp-prioritystp-优先级)
        * [`vlan`：端口 VLAN 过滤](#vlan端口-vlan-过滤)
    * [`vlan`：网桥 VLAN 过滤](#vlan网桥-vlan-过滤)
        * [`mode`：VLAN 模式](#modevlan-模式)
        * [`tag`：原生 VLAN 标签](#tag原生-vlan-标签)
        * [`enable-native`：启用原生 VLAN](#enable-native启用原生-vlan)
        * [`trunk-tags`：Trunk 标签](#trunk-tagstrunk-标签)

<!-- vim-markdown-toc -->
# Linux 网桥

Linux 网桥接口配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: br0
  type: linux-bridge
  state: up
  bridge:
    options:
      group-addr: 01:80:C2:00:00:00
      group-fwd-mask: 0
      hash-max: 4096
      mac-ageing-time: 300
      multicast-last-member-count: 2
      multicast-last-member-interval: 100
      multicast-membership-interval: 26000
      multicast-querier: false
      multicast-querier-interval: 25500
      multicast-query-interval: 12500
      multicast-query-response-interval: 1000
      multicast-query-use-ifaddr: false
      multicast-router: auto
      multicast-snooping: true
      multicast-startup-query-count: 2
      multicast-startup-query-interval: 3125
      stp:
        enabled: true
        forward-delay: 15
        hello-time: 2
        max-age: 20
        priority: 32768
      vlan-protocol: 802.1q
      vlan-default-pvid: 1
    ports:
    - name: eth1
      stp-hairpin-mode: false
      stp-path-cost: 100
      stp-priority: 32
    - name: eth2
      stp-hairpin-mode: false
      stp-path-cost: 100
      stp-priority: 32
```

## `options`：网桥选项

Linux 网桥内核选项。应用时，现有选项会合并到期望配置中。

### `group-addr`：组播组地址

网桥用于 STP 的组播 MAC 地址。必须是形如 `01:80:C2:00:00:0X`（X 为
[0, 4..f]）的链路本地地址。默认为 `01:80:C2:00:00:00`。

### `group-fwd-mask`：组转发掩码

也可配置为 `group-forward-mask`（已弃用的别名）。定义链路本地帧转发的掩码。
设置某一位将启用具有相应目标 MAC 地址的帧的转发。

### `hash-max`：哈希表最大值

组播哈希表的最大大小。必须是 2 的幂。默认为 4096。

### `mac-ageing-time`：MAC 老化时间

MAC 地址老化时间（秒）。控制在未刷新的情况下，已学习的 MAC 地址在转发
数据库中保留多长时间。特殊值：`0` 禁用老化（条目永不过期），`1` 使条目
立即消失。默认为 300。

### `multicast-last-member-count`：最后成员查询次数

收到离开消息后发送的查询次数。

### `multicast-last-member-interval`：最后成员查询间隔

最后成员查询发送之间的间隔（毫秒）。

### `multicast-membership-interval`：成员资格间隔

组播成员资格过期的时间间隔（毫秒）。

### `multicast-querier`：组播查询器

当设置为 `true` 时，网桥可以充当组播查询器。

### `multicast-querier-interval`：查询器间隔

当在此时长（毫秒）内未看到其他组播查询器发送的查询时，网桥开始发送
自己的查询。默认为 25500。

### `multicast-query-interval`：查询间隔

通用组播查询之间的间隔（毫秒）。

### `multicast-query-response-interval`：查询响应间隔

组播查询的最大响应时间（毫秒）。

### `multicast-query-use-ifaddr`：查询使用接口地址

当设置为 `true` 时，网桥使用自己的 IP 地址作为组播查询的源地址。

### `multicast-router`：组播路由器类型

组播路由器类型：
 * `auto`（1）：网桥自动检测组播路由器。
 * `disabled`（0）：组播路由器功能已禁用。
 * `enabled`（2）：网桥充当组播路由器。

### `multicast-snooping`：组播监听

当设置为 `true` 时，网桥执行 IGMP/MLD 监听以减少组播流量。

### `multicast-startup-query-count`：启动查询次数

网桥启动时发送的查询次数。

### `multicast-startup-query-interval`：启动查询间隔

启动查询之间的间隔（毫秒）。

### `stp`：生成树协议

网桥的 STP 选项。

#### `enabled`：STP 启用

启用或禁用网桥上的生成树协议。禁用时，剩余的 STP 选项在应用时会被丢弃。

#### `forward-delay`：转发延迟

转发延迟（秒）。有效范围为 2 到 30。

#### `hello-time`：Hello 时间

STP Hello BPDU 发送之间的间隔（秒）。有效范围为 1 到 10。

#### `max-age`：最大老化时间

STP 信息的最大老化时间（秒）。有效范围为 6 到 40。

#### `priority`：网桥优先级

STP 网桥优先级。较低的优先级会增加成为根网桥的机会。

### `vlan-protocol`：VLAN 协议

网桥使用的 VLAN 封装协议：
 * `802.1q`：标准 IEEE 802.1Q VLAN 标记（默认）。
 * `802.1ad`：运营商桥接（Q-in-Q）IEEE 802.1ad。

### `vlan-default-pvid`：默认 PVID

分配给端口的默认端口 VLAN ID（PVID）。默认为 `1`。设置为 `0` 将使所有
端口没有默认 PVID（它们将不接受无标签的 VLAN 流量）。除非启用了
VLAN 过滤，否则不能更改为 `1` 以外的值。

## `ports`：网桥端口

网桥端口配置列表。应用时，期望的端口列表将覆盖当前的端口列表。

### `name`：端口名称

网桥端口的内核接口名称。必填。

### `stp-hairpin-mode`：Hairpin 模式

当设置为 `true` 时，流量可以从接收到的端口发送回去。

### `stp-path-cost`：STP 路径成本

端口的 STP 路径成本。用于根端口和指定端口选择。

### `stp-priority`：STP 优先级

STP 端口优先级。无符号 8 位值（0 到 255）。较低的优先级会增加成为
指定端口的机会。

### `vlan`：端口 VLAN 过滤

特定于此端口的 VLAN 过滤配置。如果未定义，端口将保留当前的 VLAN 过滤配置。

## `vlan`：网桥 VLAN 过滤

网桥本身的 VLAN 过滤配置。设置为 `vlan: {}` 将删除所有 VLAN。

### `mode`：VLAN 模式

网桥 VLAN 过滤模式：
 * `access`：单个无标签 VLAN（Access 端口）。
 * `trunk`：有标签 VLAN（Trunk 端口）。

如果未定义，默认为 `access`。

### `tag`：原生 VLAN 标签

原生 VLAN 的 VLAN 标签。在 `access` 模式下，这是 Access VLAN。
在 `trunk` 模式下，需要将 `enable-native` 设置为 `true`。

### `enable-native`：启用原生 VLAN

当设置为 `true` 时，`tag` VLAN 被视为 Trunk 端口上的原生无标签 VLAN。
不能在 `access` 模式下设置。

### `trunk-tags`：Trunk 标签

Trunk 端口上允许的 VLAN 列表。每个条目可以是单个 VLAN ID 或范围：

```yaml
trunk-tags:
- id: 100
- id-range:
    min: 200
    max: 300
```

不允许重叠的 Trunk 标签。
