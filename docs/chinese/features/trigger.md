<!-- vim-markdown-toc GFM -->

* [条件化网络启停](#条件化网络启停)
    * [示例：基于载体的启停](#示例基于载体的启停)
    * [示例：不在家庭 WIFI 时启动 VPN](#示例不在家庭-wifi-时启动-vpn)

<!-- vim-markdown-toc -->

# 条件化网络启停

要条件化地启用/禁用接口，使用 `trigger` 部分，仅在守护进程模式下可用。

`trigger` 接受以下值：

* `never`：从不自动启停接口。
* `always`：始终启停接口，无论载体状态如何。
* `carrier`：如果载体正常，启用接口；否则禁用接口。
* `wifi: <SSID>`：如果指定的 SSID 已连接，启用接口；否则断开连接。
* `wifi-not: <SSID>`：如果指定的 SSID 已连接，禁用接口；否则连接。

## 示例：基于载体的启停

```yaml
interfaces:
- name: enp7s0
  type: ethernet
  state: up
  trigger: carrier
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: 192.0.2.251
      prefix-length: 32
  ipv6:
    enabled: false
```

## 示例：不在家庭 WIFI 时启动 VPN

```yaml
routes:
  config:
  - destination: 203.0.113.0/24
    next-hop-interface: wg0
    next-hop-address: 198.51.100.1
    metric: 100
    table-id: 25
interfaces:
- name: wg0
  type: wireguard
  state: up
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: 198.51.100.9
      prefix-length: 24
  trigger:
    wifi-not: HomeWifi
  wireguard:
    public-key: JKossUAjywXuJ2YVcaeD6PaHs+afPmIthDuqEVlspwA=
    private-key: 6LTHiAM4vgKEgi5vm30f/EBIEWFDmySkTc9EWCcIqEs=
    listen-port: 51820
    peers:
    - endpoint: 192.0.2.0:51820
      public-key: 8bdQrVLqiw3ZoHCucNh1YfH0iCWuyStniRr8t7H24Fk=
      persistent-keepalive: 0
      allowed-ips:
      - ip: 0.0.0.0
        prefix-length: 0
```
