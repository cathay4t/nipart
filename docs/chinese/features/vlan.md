<!-- vim-markdown-toc GFM -->

* [Vlan](#vlan)
    * [`base-iface`：基础接口](#base-iface基础接口)
    * [`id`：VLAN ID](#idvlan-id)
    * [`protocol`：VLAN 协议](#protocolvlan-协议)
    * [`registration-protocol`：VLAN 注册协议](#registration-protocolvlan-注册协议)
    * [`reorder-headers`：重排输出数据包头](#reorder-headers重排输出数据包头)
    * [`loose-binding`：松散绑定](#loose-binding松散绑定)
    * [`bridge-binding`：网桥绑定](#bridge-binding网桥绑定)
    * [`ingress-qos-map`：入口 QoS 映射](#ingress-qos-map入口-qos-映射)
    * [`egress-qos-map`：出口 QoS 映射](#egress-qos-map出口-qos-映射)

<!-- vim-markdown-toc -->

# Vlan

VLAN 接口配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: eth1.101
  type: vlan
  state: up
  vlan:
    base-iface: eth1
    id: 101
    protocol: 802.1q
    registration-protocol: none
    reorder-headers: true
    loose-binding: false
    bridge-binding: false
    ingress-qos-map:
    - from: 3
      to: 1
    egress-qos-map:
    - from: 1
      to: 3
```

## `base-iface`：基础接口

创建 VLAN 的物理或父接口名称，例如 `eth1`。

创建新 VLAN 接口时必填。对现有 VLAN 应用更改时，留空将保留当前的基础接口。

## `id`：VLAN ID

VLAN 标识符。有效范围是 0 到 4094。

创建新 VLAN 接口时必填。对现有 VLAN 应用更改时，留空将保留当前的 ID。

## `protocol`：VLAN 协议

VLAN 封装协议：
 * `802.1q`：标准 IEEE 802.1Q VLAN 标记（默认）。
 * `802.1ad`：运营商桥接（Q-in-Q）IEEE 802.1ad。

如果未定义，默认为 `802.1q`。

## `registration-protocol`：VLAN 注册协议

用于 VLAN 修剪的注册协议：
 * `gvrp`：GARP VLAN 注册协议。
 * `mvrp`：多 VLAN 注册协议。
 * `none`：无注册协议（默认）。

## `reorder-headers`：重排输出数据包头

当设置为 `true` 时，VLAN 设备将重排输出数据包的头部，将 VLAN 标签移到任何
硬件特定头部之前。默认为 `true`。

## `loose-binding`：松散绑定

当设置为 `true` 时，VLAN 设备以松散绑定模式运行，在此模式下 VLAN 设备状态
不严格绑定到主设备的运行状态。

## `bridge-binding`：网桥绑定

当设置为 `true` 时，VLAN 设备的链路状态跟踪作为 VLAN 成员的网桥端口的状态。

## `ingress-qos-map`：入口 QoS 映射

将入口数据包的 VLAN 头部 PCP（优先级代码点）值映射到 Linux 内部数据包
优先级。每个条目将 `from`（VLAN PCP 值）映射到 `to`（Linux 优先级）。

根据 802.1Q-2018 PCP 字段定义，最大优先级值为 7。

## `egress-qos-map`：出口 QoS 映射

将 Linux 内部数据包优先级映射到出口数据包的 VLAN 头部 PCP 值。
每个条目将 `from`（Linux 优先级）映射到 `to`（VLAN PCP 值）。

根据 802.1Q-2018 PCP 字段定义，最大优先级值为 7。
