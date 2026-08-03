# Nipart

Nipart 是基于 Rust 的 Linux 网络管理工具，提供内存安全、线程安全的守护进程、命令行工具和 API 库。

面向企业场景中网桥、VLAN、Bond、VRF 等复杂网络拓扑，Nipart 提供声明式
YAML 极大简化网络部署复杂度。

Nipart 支持无守护进程模式，可快速网络配置，且不留下任何残留进程或文件，
匹配云原生容器环境用例。

提供完善插件接口，支持自定义 VPN 或第三方网卡管理插件。

## YAML API 示例

`npt show` 输出示例：

```bash
---
version: 1
routes:
  config:
  - destination: 0.0.0.0/0
    next-hop-interface: bond1
    next-hop-address: 192.0.2.1
    table-id: 254
    metric: 1000
interfaces:
  - name: bond1
    type: bond
    state: up
    ipv4:
      address:
      - ip: 192.0.2.252
        prefix-length: 24
      dhcp: false
      enabled: true
    bond:
      mode: active-backup
      ports:
        - name: port1
        - name: port2
  - name: port1
    type: ethernet
    state: up
    mac-address: 00:23:45:67:89:1a
    identifier: mac-address
  - name: port2
    type: ethernet
    state: up
    mac-address: 00:23:45:67:89:1b
    identifier: mac-address
```


## 功能特性

* [基础接口管理](docs/chinese/features/base.md)
* [IP 地址](docs/chinese/features/ip.md)
* [无守护进程模式](docs/chinese/features/no_daemon_mode.md)
* [WIFI](docs/chinese/features/wifi.md)
* [路由](docs/chinese/features/route.md)
* [条件化网络启停](docs/chinese/features/auto_connect.md)
* [网络就绪等待](docs/chinese/features/wait-online.md)
* [Vlan](docs/chinese/features/vlan.md)
* [VxLAN](docs/chinese/features/vxlan.md)
* [Bond](docs/chinese/features/bond.md)
* [Linux 网桥](docs/chinese/features/bridge.md)
* [OpenvSwitch 网桥](docs/chinese/features/ovs.md)
* [Wireguard](docs/chinese/features/wireguard.md)

## 安装

### 从源码构建
```bash
cargo build --release
sudo systemctl stop nipart || true
sudo cp -fv target/release/nipartd /usr/bin/
sudo cp -fv target/release/npt /usr/bin/
sudo cp -fv packaging/nipart.service /etc/systemd/system/
sudo cp -fv packaging/nipart-wait-online.service /etc/systemd/system/
sudo systemctl enable nipart.service
sudo systemctl enable nipart-wait-online.service
sudo systemctl start nipart.service
```

### 从 Archlinux AUR 安装

TODO: 上传至 AUR

### 从 Fedora COPR 安装

TODO: 上传至 COPR

## 使用

### 显示当前网络状态

```bash
# 守护进程模式
sudo npt show
# 无守护进程模式
sudo npt show -n
```

### 显示守护进程保存的配置

```bash
sudo npt show -s
```

### 显示特定接口的运行状态

```bash
sudo npt show wlan0
```

### 扫描 WIFI 网络

```bash
sudo npt wifi scan
```

### 连接 WIFI

```bash
# 此命令会要求你输入 WiFi 密码
sudo npt wifi connect <SSID>
```
