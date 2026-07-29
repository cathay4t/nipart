<!-- vim-markdown-toc GFM -->

* [MAC Address Identifier](#mac-address-identifier)
    * [Example: Identify interface by MAC address](#example-identify-interface-by-mac-address)
        * [What happens behind the scenes](#what-happens-behind-the-scenes)
    * [Example: Route using logical interface name](#example-route-using-logical-interface-name)
    * [Example: Remove interface and route by MAC address](#example-remove-interface-and-route-by-mac-address)
    * [How it works](#how-it-works)
    * [Limitations](#limitations)

<!-- vim-markdown-toc -->

# MAC Address Identifier

When the kernel interface name is unpredictable (e.g. after NIC replacement or
kernel upgrade), you can use `identifier: mac-address` to match an interface
by its MAC address instead of its kernel name.

## Example: Identify interface by MAC address

```yaml
---
interfaces:
  - name: my-veth
    type: ethernet
    identifier: mac-address
    mac-address: 52:54:00:12:AF:0B
    state: up
    ipv4:
      enabled: true
      dhcp: false
      address:
        - ip: 192.0.2.99
          prefix-length: 24
```

In this example, `my-veth` is the logical name used for referencing this
interface across multiple applies. The actual kernel interface will be
identified by the provided MAC address.

### What happens behind the scenes

On apply:

1. Nipart scans the current network state for an interface holding the
   specified MAC address.
2. The `name` and `kernel-iface-name` of the desired interface are
   overwritten with the found kernel interface name.
3. The original logical name is preserved as `profile-name`.
4. The interface type is resolved from `ethernet` to the actual kernel
   interface type when `type: unknown` is used.

## Example: Route using logical interface name

The logical name can also be used as `next-hop-interface` in routes. Nipart
will resolve it to the actual kernel interface name:

```yaml
---
interfaces:
  - name: my-gw-iface
    type: ethernet
    identifier: mac-address
    mac-address: 52:54:00:12:AF:0B
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

## Example: Remove interface and route by MAC address

The `identifier: mac-address` can also be used with `state: absent` to remove
the stored profile and route configuration associated with the logical name.
Absent interfaces are skipped during MAC resolution and matched by their
logical name instead:

```yaml
---
interfaces:
  - name: my-gw-iface
    type: ethernet
    identifier: mac-address
    mac-address: 52:54:00:12:AF:0B
    state: absent
routes:
  config:
    - destination: 0.0.0.0/0
      next-hop-interface: my-gw-iface
      next-hop-address: 198.51.100.254
      state: absent
      table-id: 254
```

## How it works

The `identifier: mac-address` property can be used with these interface types:

* `type: ethernet` (most common)
* `type: unknown` (when the interface type is not known in advance)

Key points:

* The `mac-address` field is required when using `identifier: mac-address`.
* Both `mac-address` and `permanent-mac-address` of the current state are
  checked, with `permanent-mac-address` preferred.
* The MAC address matching is case-insensitive.
* Interfaces with `state: absent` are skipped and not resolved.
* When `type: unknown` is used, the interface type is automatically
  resolved to the actual type of the matched kernel interface.

## Limitations

* The `identifier: mac-address` is only supported on ethernet and unknown
  interface types.
* The matched kernel interface must exist in the current running state.
* If multiple interfaces share the same MAC address, the first match is
  used.
* This feature only works in daemon mode, as it requires querying the
  current network state.
