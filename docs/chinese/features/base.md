<!-- vim-markdown-toc GFM -->

* [Nipart 基础接口支持](#nipart-基础接口支持)
    * [`name`：接口名称](#name接口名称)
    * [`type`：接口类型](#type接口类型)
    * [`description` - 接口描述](#description---接口描述)
    * [`iface-index` - 接口索引](#iface-index---接口索引)
    * [`state` - 接口状态](#state---接口状态)
    * [`link-state`：链路状态](#link-state链路状态)
    * [`controller` - 控制器接口](#controller---控制器接口)
    * [`mac-address` - MAC 地址](#mac-address---mac-地址)
    * [`permanent-mac-address` - 永久 MAC 地址](#permanent-mac-address---永久-mac-地址)
    * [`mtu` - MTU](#mtu---mtu)
    * [`min-mtu` - 最小 MTU](#min-mtu---最小-mtu)
    * [`max-mtu` - 最大 MTU](#max-mtu---最大-mtu)
    * [`ipv4` 和 `ipv6` - IP 配置](#ipv4-和-ipv6---ip-配置)

<!-- vim-markdown-toc -->

# Nipart 基础接口支持

Nipart 支持的所有接口类型都支持以下 YAML 结构：

```yaml
---
version: 1
interfaces:
- name: eth1
  type: veth
  description: Main interface connected to switch S1
  iface-index: 8
  state: up
  link-state: up
  controller: bond0
  mac-address: 02:00:00:00:00:0f
  permanent-mac-address: 02:00:00:00:00:0f
  mtu: 1500
  min-mtu: 68
  max-mtu: 65535
  ipv4:
    enabled: false
  ipv6:
    enabled: false
```


## `name`：接口名称

接口的内核名称。

## `type`：接口类型

接口的类型，例如 `veth`、`bond`、`bridge` 等。

## `description` - 接口描述

保存专用属性。该属性会保存在已保存状态中，并会在运行状态查询中显示，
但不会应用到内核，也不会在验证时检查。

将其设置为空字符串可清除已保存的描述。

## `iface-index` - 接口索引

接口的内核索引。仅查询属性。

## `state` - 接口状态

接口的管理状态：
 * `up`：接口已管理性启用
 * `down`：接口已管理性禁用
 * `absent`：仅应用属性，请求 nipart 删除此接口或将物理接口恢复到内核默认状态
 * `ignore`：不受 nipart 管理
 * `up-ignore`：接口已管理性启用但不受 nipart 管理
 * `down-ignore`：接口已管理性禁用但不受 nipart 管理

## `link-state`：链路状态

仅查询属性。接口的载体状态：
 * `up`：接口有载体信号
 * `down`：接口无载体信号
 * `dormant`：接口处于休眠状态，例如无线接口处于省电模式
 * `lower-layer-down`：接口因下层故障而关闭，例如物理接口断开
 * `testing`：接口处于测试模式

## `controller` - 控制器接口

控制器接口的内核接口名称。

应用时，将此属性设置为空字符串表示从当前控制器分离。
应用时，将此属性设置为非空字符串表示附加到指定的控制器接口。

## `mac-address` - MAC 地址

接口的当前 MAC 地址。

应用时，将此属性设置为空字符串表示恢复永久 MAC 地址。
应用时，将此属性设置为非空字符串表示设置 MAC 地址。

## `permanent-mac-address` - 永久 MAC 地址

仅查询属性。接口的永久 MAC 地址。

## `mtu` - MTU

接口的 MTU。

## `min-mtu` - 最小 MTU

仅查询属性。接口支持的最小 MTU。

## `max-mtu` - 最大 MTU

仅查询属性。接口支持的最大 MTU。

## `ipv4` 和 `ipv6` - IP 配置

详情请参见 [IP 配置](./ip.md)。
