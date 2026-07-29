<!-- vim-markdown-toc GFM -->

* [OpenvSwitch 网桥](#openvswitch-网桥)
    * [`bridge`](#bridge)
        * [`ports`：网桥端口](#ports网桥端口)
            * [`name`：端口名称](#name端口名称)
    * [端口接口](#端口接口)
    * [OVS 内部接口](#ovs-内部接口)
    * [限制](#限制)

<!-- vim-markdown-toc -->

# OpenvSwitch 网桥

OVS 网桥配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: br0
  type: ovs-bridge
  state: up
  bridge:
    ports:
    - name: eth1
    - name: eth2
- name: br0
  type: ovs-interface
  state: up
  controller: br0
  ipv4:
    enabled: false
  ipv6:
    enabled: false
- name: eth1
  type: ovs-interface
  state: up
  controller: br0
- name: eth2
  type: ovs-interface
  state: up
  controller: br0
```

OVS 网桥由三部分组成：`ovs-bridge` 接口本身、一个用于网桥内部接口的
`ovs-interface`，以及每个端口的 `ovs-interface` 条目。

## `bridge`

OVS 网桥配置。

### `ports`：网桥端口

附加到网桥的端口名称列表。每个端口条目包含：

#### `name`：端口名称

附加到此 OVS 网桥的端口接口名称。对应的接口应定义为
`type: ovs-interface` 且 `controller: <bridge-name>`。

## 端口接口

端口定义为单独的接口，`type` 为 `ovs-interface`，`controller` 设置为
OVS 网桥名称。它们继承所有基础接口属性，包括 IP 配置。

## OVS 内部接口

网桥本身通常有一个与网桥同名的 `ovs-interface`，作为网桥的内部接口并
配置 IP。

## 限制

* 尚不支持 OVS Bond。
* 尚不支持 OVS `patch` 和 `dpdk` 接口类型。
