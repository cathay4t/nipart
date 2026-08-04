<!-- vim-markdown-toc GFM -->

* [VxLAN](#vxlan)
    * [`base-iface`: Base interface](#base-iface-base-interface)
    * [`id`: VxLAN ID (VNI)](#id-vxlan-id-vni)
    * [`remote`: Remote tunnel endpoint](#remote-remote-tunnel-endpoint)
    * [`local`: Local tunnel endpoint](#local-local-tunnel-endpoint)
    * [`destination-port`: Destination port](#destination-port-destination-port)
    * [`learning`: FDB learning](#learning-fdb-learning)
    * [`ttl`: IP TTL](#ttl-ip-ttl)
    * [`tos`: IP TOS](#tos-ip-tos)
    * [`ageing`: FDB entry ageing time](#ageing-fdb-entry-ageing-time)
    * [`max-address`: Maximum FDB entries](#max-address-maximum-fdb-entries)
    * [`src-port-min` / `src-port-max`: Source port range](#src-port-min--src-port-max-source-port-range)
    * [`proxy`: ARP proxy](#proxy-arp-proxy)
    * [`rsc`: Route short circuit](#rsc-route-short-circuit)
    * [`l2miss`: L2 miss notification](#l2miss-l2-miss-notification)
    * [`l3miss`: L3 miss notification](#l3miss-l3-miss-notification)
    * [`udp-check-sum`: UDP checksum](#udp-check-sum-udp-checksum)
    * [`udp6-zero-check-sum-tx`: IPv6 UDP zero checksum TX](#udp6-zero-check-sum-tx-ipv6-udp-zero-checksum-tx)
    * [`udp6-zero-check-sum-rx`: IPv6 UDP zero checksum RX](#udp6-zero-check-sum-rx-ipv6-udp-zero-checksum-rx)
    * [`remote-check-sum-tx`: Remote checksum TX](#remote-check-sum-tx-remote-checksum-tx)
    * [`remote-check-sum-rx`: Remote checksum RX](#remote-check-sum-rx-remote-checksum-rx)
    * [`gbp`: Group Based Policy](#gbp-group-based-policy)
    * [`remote-check-sum-no-partial`: Remote checksum no partial](#remote-check-sum-no-partial-remote-checksum-no-partial)
    * [`collect-metadata`: Collect metadata](#collect-metadata-collect-metadata)
    * [`label`: Flow label](#label-flow-label)
    * [`gpe`: Generic Protocol Extension](#gpe-generic-protocol-extension)
    * [`ttl-inherit`: TTL inherit](#ttl-inherit-ttl-inherit)

<!-- vim-markdown-toc -->

# VxLAN

> **Note:** The following properties can be changed on a live interface without
> deletion:
>  * `remote`
>  * `local`
>  * `learning`
>  * `ttl`
>  * `tos`
>  * `ageing`
>  * `label`
>
> All other VxLAN properties changes will trigger interface deletion and
> recreation.

Example YAML of VxLAN interface configuration:

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

## `base-iface`: Base interface

The physical or parent interface name on which the VxLAN tunnel is created,
e.g. `eth1`. The VxLAN will encapsulate traffic over this interface.

Mandatory when creating a new VxLAN interface. When applying changes to an
existing VxLAN, leaving this unset preserves the current base interface.

## `id`: VxLAN ID (VNI)

The VxLAN Network Identifier (VNI). Valid range is 0 to 16777215 (24-bit).

Mandatory when creating a new VxLAN interface. When applying changes to an
existing VxLAN, leaving this unset preserves the current ID.

## `remote`: Remote tunnel endpoint

The unicast or multicast IP address of the remote VXLAN Tunnel Endpoint (VTEP),
e.g. `192.0.2.251` or `2001:db8::1`.

## `local`: Local tunnel endpoint

The IP address of the local VXLAN Tunnel Endpoint (VTEP), e.g. `192.0.2.252` or
`2001:db8::2`.

## `destination-port`: Destination port

The UDP destination port for VxLAN communication. Defaults to `4789` (IANA
assigned VxLAN port) if not defined.

## `learning`: FDB learning

When set to `true`, the bridge's VXLAN learning is enabled, allowing the kernel
to populate the FDB automatically. Defaults to `true` if not defined.

## `ttl`: IP TTL

The TTL value used for the VxLAN tunnel protocol IP header.

## `tos`: IP TOS

The TOS (Type of Service) value used for the VxLAN tunnel protocol IP header.

## `ageing`: FDB entry ageing time

The lifetime in seconds of FDB entries learned by the kernel.

## `max-address`: Maximum FDB entries

The maximum number of FDB entries allowed for this VxLAN interface.

## `src-port-min` / `src-port-max`: Source port range

The range of UDP source ports used for VxLAN communication. Both `src-port-min`
and `src-port-max` must be specified together to define the range.

## `proxy`: ARP proxy

When set to `true`, ARP proxy is enabled on the VxLAN interface.

## `rsc`: Route short circuit

When set to `true`, route short circuit is enabled.

## `l2miss`: L2 miss notification

When set to `true`, netlink notifications are generated for L2 address lookup
misses in the FDB.

## `l3miss`: L3 miss notification

When set to `true`, netlink notifications are generated for L3 address lookup
misses in the FDB.

## `udp-check-sum`: UDP checksum

When set to `true`, UDP checksum computation is enabled for the VxLAN tunnel.

## `udp6-zero-check-sum-tx`: IPv6 UDP zero checksum TX

When set to `true`, sending UDP packets with zero checksum is allowed for IPv6
tunnels.

## `udp6-zero-check-sum-rx`: IPv6 UDP zero checksum RX

When set to `true`, receiving UDP packets with zero checksum is allowed for
IPv6 tunnels.

## `remote-check-sum-tx`: Remote checksum TX

When set to `true`, remote checksum offload for transmission is enabled.

## `remote-check-sum-rx`: Remote checksum RX

When set to `true`, remote checksum offload for reception is enabled.

## `gbp`: Group Based Policy

When set to `true`, Group Based Policy extension is enabled.

## `remote-check-sum-no-partial`: Remote checksum no partial

When set to `true`, partial remote checksum is disabled.

## `collect-metadata`: Collect metadata

When set to `true`, the VxLAN interface collects metadata from incoming packets.

## `label`: Flow label

The IPv6 flow label for the VxLAN tunnel. For IPv6 only.

## `gpe`: Generic Protocol Extension

When set to `true`, the Generic Protocol Extension (GPE) is enabled, allowing
other protocols beside Ethernet to be carried.

## `ttl-inherit`: TTL inherit

When set to `true`, the VxLAN tunnel inherits the TTL from the inner packet.
