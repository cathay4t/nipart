<!-- vim-markdown-toc GFM -->

* [Conditional Network Up/Down](#conditional-network-updown)
    * [Example: Carrier-based up/down](#example-carrier-based-updown)
    * [Example: Start VPN when not in home WIFI](#example-start-vpn-when-not-in-home-wifi)

<!-- vim-markdown-toc -->

# Conditional Network Up/Down

For conditionally bringing an interface up/down, the `auto-connect` section
is used, daemon mode only.

The `auto-connect` takes these values:

* `true`: Activate the interface automatically:
   * For physical interface, apply the config upon carrier up.
   * For virtual interface(e.g. bond, VLAN, linux bridge), apply the config
     upon boot or apply action.
* `false`: Only apply the config upon apply action, ignored in boot action.
* `wifi: <SSID>`: Bring interface up if specified SSID is connected,
  otherwise disconnect.
* `wifi-not: <SSID>`: Bring interface down if specified SSID is connected,
  otherwise connect.

When not defined, it defaults to `true`.

## Example: Carrier-based up/down

```yaml
interfaces:
- name: enp7s0
  type: ethernet
  state: up
  auto-connect: true
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: 192.0.2.251
      prefix-length: 32
  ipv6:
    enabled: false
```

## Example: Start VPN when not in home WIFI

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
  auto-connect:
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
