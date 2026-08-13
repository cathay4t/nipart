<!-- vim-markdown-toc GFM -->

* [MAC 地址标识符](#mac-地址标识符)
    * [示例：通过 MAC 地址识别接口](#示例通过-mac-地址识别接口)
        * [幕后发生了什么](#幕后发生了什么)
    * [示例：使用逻辑接口名称配置路由](#示例使用逻辑接口名称配置路由)
    * [示例：通过 MAC 地址删除接口和路由](#示例通过-mac-地址删除接口和路由)
    * [工作原理](#工作原理)
    * [限制](#限制)

<!-- vim-markdown-toc -->

# MAC 地址标识符

当内核接口名称不可预测时（例如更换网卡或内核升级后），你可以使用
`identifier: mac-address` 通过 MAC 地址而非内核名称来匹配接口。

## 示例：通过 MAC 地址识别接口

```yaml
---
interfaces:
  - name: my-veth
    type: ethernet
    identifier: mac-address
    mac-address: 02:00:00:00:00:0b
    state: up
    ipv4:
      enabled: true
      dhcp: false
      address:
        - ip: 192.0.2.99
          prefix-length: 24
```

在此示例中，`my-veth` 是在多次应用间引用此接口所用的逻辑名称。
实际的内核接口将通过提供的 MAC 地址来识别。

### 幕后发生了什么

应用时：

1. Nipart 扫描当前网络状态，查找持有指定 MAC 地址的接口。
2. 期望接口的 `name` 和 `kernel-iface-name` 会被找到的内核接口名称覆盖。
3. 原始逻辑名称被保留为 `profile-name`。
4. 当使用 `type: unknown` 时，接口类型会从 `ethernet` 解析为匹配到的实际内核
   接口类型。

## 示例：使用逻辑接口名称配置路由

逻辑名称也可以在路由中用作 `next-hop-interface`。Nipart 会将其解析为实际的
内核接口名称：

```yaml
---
interfaces:
  - name: my-gw-iface
    type: ethernet
    identifier: mac-address
    mac-address: 02:00:00:00:00:0b
    state: up
    ipv4:
      enabled: true
      dhcp: false
routes:
  config:
    - destination: 0.0.0.0/0
      next-hop-interface: my-gw-iface
      next-hop-address: 198.51.100.254
      table-id: 254
```

## 示例：通过 MAC 地址删除接口和路由

`identifier: mac-address` 也可以配合 `state: absent` 使用，来删除与该逻辑名称
关联的已存储配置文件和路由配置。状态为 absent 的接口在 MAC 解析期间会被跳过，
改为通过其逻辑名称进行匹配：

```yaml
---
interfaces:
  - name: my-gw-iface
    type: ethernet
    identifier: mac-address
    mac-address: 02:00:00:00:00:0b
    state: absent
routes:
  config:
    - destination: 0.0.0.0/0
      next-hop-interface: my-gw-iface
      next-hop-address: 198.51.100.254
      state: absent
      table-id: 254
```

## 工作原理

`identifier: mac-address` 属性可用于以下接口类型：

* `type: ethernet`（最常见）
* `type: unknown`（当接口类型事先未知时）

关键点：

* 使用 `identifier: mac-address` 时，`mac-address` 字段是必需的。
* 当前状态的 `mac-address` 和 `permanent-mac-address` 都会被检查，
  优先使用 `permanent-mac-address`。
* MAC 地址匹配不区分大小写。
* 状态为 `absent` 的接口会被跳过，不进行解析。
* 当使用 `type: unknown` 时，接口类型会自动解析为匹配到的内核接口的实际类型。

## 限制

* `identifier: mac-address` 仅支持以太网和未知接口类型。
* 匹配到的内核接口必须存在于当前运行状态中。
* 如果多个接口共享相同的 MAC 地址，将使用第一个匹配项。
* 当匹配到的接口被 bond 托管（作为 bond 端口）时，`mac-address` 仅作为
  标识符使用，不会被应用或校验，因为 bond 内核驱动控制其端口的 MAC 地址
  （例如 active-backup 模式下内核会将 bond 的 MAC 分配给每个 slave）。
* 此功能仅在守护进程模式下有效，因为它需要查询当前网络状态。
