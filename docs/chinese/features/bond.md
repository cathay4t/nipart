<!-- vim-markdown-toc GFM -->

* [Bond](#bond)
    * [`mode`：Bond 模式](#modebond-模式)
    * [`options`：Bond 选项](#optionsbond-选项)
        * [`miimon`：MII 链路监控间隔](#miimonmii-链路监控间隔)
        * [`updelay`：启用延迟](#updelay启用延迟)
        * [`downdelay`：禁用延迟](#downdelay禁用延迟)
        * [`use_carrier`：使用载体](#use_carrier使用载体)
        * [`arp_interval`：ARP 监控间隔](#arp_intervalarp-监控间隔)
        * [`arp_ip_target`：ARP 目标 IP](#arp_ip_targetarp-目标-ip)
        * [`arp_all_targets`：ARP 全部目标](#arp_all_targetsarp-全部目标)
        * [`arp_validate`：ARP 验证](#arp_validatearp-验证)
        * [`arp_missed_max`：ARP 最大未命中数](#arp_missed_maxarp-最大未命中数)
        * [`fail_over_mac`：故障切换 MAC](#fail_over_mac故障切换-mac)
        * [`primary`：主端口](#primary主端口)
        * [`primary_reselect`：主端口重新选择策略](#primary_reselect主端口重新选择策略)
        * [`lacp_rate`：LACP 速率](#lacp_ratelacp-速率)
        * [`lacp_active`：LACP 主动模式](#lacp_activelacp-主动模式)
        * [`xmit_hash_policy`：发送哈希策略](#xmit_hash_policy发送哈希策略)
        * [`ad_select`：802.3ad 聚合选择](#ad_select8023ad-聚合选择)
        * [`ad_actor_sys_prio`：Actor 系统优先级](#ad_actor_sys_prioactor-系统优先级)
        * [`ad_actor_system`：Actor 系统 MAC](#ad_actor_systemactor-系统-mac)
        * [`ad_user_port_key`：用户端口密钥](#ad_user_port_key用户端口密钥)
        * [`all_slaves_active`：所有从端口活跃](#all_slaves_active所有从端口活跃)
        * [`min_links`：最小链路数](#min_links最小链路数)
        * [`lp_interval`：学习包间隔](#lp_interval学习包间隔)
        * [`packets_per_slave`：每个从端口的数据包数](#packets_per_slave每个从端口的数据包数)
        * [`resend_igmp`：重发 IGMP](#resend_igmp重发-igmp)
        * [`num_grat_arp`：免费 ARP 数量](#num_grat_arp免费-arp-数量)
        * [`num_unsol_na`：非请求 NA 数量](#num_unsol_na非请求-na-数量)
        * [`peer_notif_delay`：对等方通知延迟](#peer_notif_delay对等方通知延迟)
        * [`tlb_dynamic_lb`：TLB 动态负载均衡](#tlb_dynamic_lbtlb-动态负载均衡)
        * [`ns_ip6_target`：IPv6 邻居请求目标](#ns_ip6_targetipv6-邻居请求目标)
    * [`ports`：Bond 端口](#portsbond-端口)
        * [`name`：端口名称](#name端口名称)
        * [`priority`：端口优先级](#priority端口优先级)
        * [`queue-id`：端口队列 ID](#queue-id端口队列-id)

<!-- vim-markdown-toc -->
# Bond

Bond 接口配置的 YAML 示例：

```yaml
version: 1
interfaces:
- name: bond0
  type: bond
  state: up
  bond:
    mode: 802.3ad
    options:
      miimon: 100
      updelay: 0
      downdelay: 0
      use_carrier: true
      lacp_rate: slow
      lacp_active: true
      xmit_hash_policy: layer3+4
      ad_select: stable
      ad_actor_sys_prio: 65535
      ad_user_port_key: 0
      min_links: 0
      lp_interval: 1
      packets_per_slave: 1
      resend_igmp: 1
      all_slaves_active: dropped
      arp_interval: 0
      arp_ip_target: 192.0.2.1,192.0.2.2
      arp_all_targets: any
      arp_validate: none
      arp_missed_max: 3
      fail_over_mac: none
      primary: eth1
      primary_reselect: always
      num_grat_arp: 1
      num_unsol_na: 1
      peer_notif_delay: 0
      tlb_dynamic_lb: true
      ns_ip6_target:
      - "2001:db8::1"
    ports:
    - name: eth1
      priority: 0
      queue-id: 0
    - name: eth2
      priority: 0
      queue-id: 0
```

## `mode`：Bond 模式

绑定模式。创建新 Bond 接口时必填。支持数字别名用于反序列化：

 * `balance-rr`（0）：轮询：按顺序发送数据包。
 * `active-backup`（1）：主备：一个端口活跃，备用端口在故障时接管。
 * `balance-xor`（2）：XOR：基于 MAC 地址的 XOR 结果发送。
 * `broadcast`（3）：广播：在所有端口上发送所有数据包。
 * `802.3ad`（4，别名 `lacp`）：IEEE 802.3ad 动态链路聚合。
 * `balance-tlb`（5）：自适应发送负载均衡。
 * `balance-alb`（6）：自适应负载均衡（TLB + 接收负载均衡）。

## `options`：Bond 选项

内核 Bond 选项。详情请参考内核文档。

### `miimon`：MII 链路监控间隔

MII 链路监控的间隔（毫秒）。默认为 0（禁用）。不能与 `arp_interval` 同时使用。

### `updelay`：启用延迟

检测到链路启用后，启用端口前的延迟（毫秒）。默认为 0。仅当 `miimon`
启用（大于 0）时才能设置；该值必须是 `miimon` 的倍数，否则内核会
向下取整。

### `downdelay`：禁用延迟

检测到链路丢失后，禁用端口前的延迟（毫秒）。默认为 0。仅当 `miimon`
启用（大于 0）时才能设置；该值必须是 `miimon` 的倍数，否则内核会
向下取整。

### `use_carrier`：使用载体

已过时的内核选项，以前用于在 MII/ETHTOOL ioctl 与 `netif_carrier_ok()`
之间选择链路状态检测方式。现在所有链路状态检查均使用
`netif_carrier_ok()`；设置此选项无效。默认为 `true`。

### `arp_interval`：ARP 监控间隔

ARP 监控的间隔（毫秒）。默认为 0（禁用）。不能与 `miimon` 同时使用。
不适用于 `802.3ad`、`balance-tlb` 和 `balance-alb` 模式。

### `arp_ip_target`：ARP 目标 IP

逗号分隔的 IPv4 地址，用作 ARP 监控目标。仅在 `arp_interval` 大于 0 时有效。

### `arp_all_targets`：ARP 全部目标

指定必须有多少个 `arp_ip_target` 可达，端口才被视为正常：
 * `any`（0）：任意单个目标可达。
 * `all`（1）：所有目标必须可达。

仅影响启用了 `arp_validate` 的 `active-backup` 模式。

### `arp_validate`：ARP 验证

指定链路监控的 ARP 探测和回复验证：
 * `none`（0）：无验证。
 * `active`（1）：仅验证活跃端口。
 * `backup`（2）：仅验证备用端口。
 * `all`（3）：验证所有端口。
 * `filter`（4）：在所有端口上过滤非 ARP 流量。
 * `filter_active`（5）：在所有端口上过滤，在活跃端口上验证。
 * `filter_backup`（6）：在所有端口上过滤，在备用端口上验证。

### `arp_missed_max`：ARP 最大未命中数

在认为链路断开之前，ARP 监控的最大未命中次数。

### `fail_over_mac`：故障切换 MAC

指定 `active-backup` 模式下的 MAC 地址处理方式：
 * `none`（0）：所有端口共享相同的 MAC 地址。
 * `active`（1）：Bond MAC 跟随活跃端口的 MAC。
 * `follow`（2）：仅在故障切换时将端口编程为 Bond MAC。

当 `fail_over_mac` 设置为 `active` 且模式为 `active-backup` 时，期望的 MAC
地址将被忽略，因为它由活跃端口决定。

### `primary`：主端口

在 `active-backup`、`balance-tlb` 和 `balance-alb` 模式下用作主端口（首选
活跃端口）的端口名称。

### `primary_reselect`：主端口重新选择策略

指定主端口何时重新变为活跃：
 * `always`（0）：主端口恢复后立即变为活跃。
 * `better`（1）：仅当主端口的速度/双工优于当前活跃端口时。
 * `failure`（2）：仅当当前活跃端口故障时。

### `lacp_rate`：LACP 速率

`802.3ad` 模式下的 LACP PDU 速率：
 * `slow`（0）：每 30 秒发送一次 LACPDU。
 * `fast`（1）：每 1 秒发送一次 LACPDU。

仅在 `802.3ad` 模式下有效。

### `lacp_active`：LACP 主动模式

当设置为 `true` 时，无论对等方状态如何都发送 LACPDU。当设置为 `false` 时，
仅在响应接收到的 LACPDU 时发送 LACPDU。仅在 `802.3ad` 模式下有效。

### `xmit_hash_policy`：发送哈希策略

`balance-xor`、`802.3ad` 和 `balance-tlb` 模式的发送哈希策略：
 * `layer2`（0）：基于 MAC 地址哈希。
 * `layer3+4`（1）：基于 IP 和端口哈希。
 * `layer2+3`（2）：基于 MAC 和 IP 哈希。
 * `encap2+3`（3）：隧道接口的封装层 2+3。
 * `encap3+4`（4）：隧道接口的封装层 3+4。
 * `vlan+srcmac`（5）：基于 VLAN 标签和源 MAC 哈希。

### `ad_select`：802.3ad 聚合选择

`802.3ad` 模式的聚合选择逻辑：
 * `stable`（0）：选择挂载端口最多的聚合器，并且仅当之前的聚合器不再有
   相关端口时才重新选择活跃聚合器。
 * `bandwidth`（1）：选择总带宽最高的聚合器。
 * `count`（2）：选择端口数量最多的聚合器。
 * `actor_port_prio`（3）：选择端口总优先级最高的聚合器。

### `ad_actor_sys_prio`：Actor 系统优先级

802.3ad Actor 系统优先级。用于 LACP 协商。

### `ad_actor_system`：Actor 系统 MAC

802.3ad Actor 系统 MAC 地址。必须是有效的单播 MAC 地址；内核拒绝任何
组播 MAC 地址。

### `ad_user_port_key`：用户端口密钥

802.3ad 用户定义的端口密钥。

### `all_slaves_active`：所有从端口活跃

也可反序列化为 `all_ports_active`。指定非活跃端口上重复帧的处理方式：
 * `dropped`（0）：丢弃重复帧。
 * `delivered`（1）：递送重复帧。

### `min_links`：最小链路数

Bond 被认为正常之前必须活跃的最小端口数。

### `lp_interval`：学习包间隔

Bond 驱动向每个端口的对端交换机发送学习数据包的时间间隔（秒）。仅在
`balance-alb` 模式下生效。默认为 1。

### `packets_per_slave`：每个从端口的数据包数

也可反序列化为 `packets_per_port`。在 `balance-rr` 模式下，切换到下一个端口
之前在一个端口上传输的数据包数。

### `resend_igmp`：重发 IGMP

故障切换后发送的 IGMP 成员报告数。

### `num_grat_arp`：免费 ARP 数量

故障切换后发送的免费 ARP 数据包数。如果同时定义了 `num_unsol_na`，
两者必须相等。

### `num_unsol_na`：非请求 NA 数量

故障切换后发送的非请求 IPv6 邻居通告数。内核中与 `num_grat_arp` 含义相同。

### `peer_notif_delay`：对等方通知延迟

故障切换后免费 ARP/NA 通知之间的延迟（毫秒）。仅当 `miimon` 启用（大于
0）时才能设置；该值必须是 `miimon` 的倍数，否则内核会向下取整。

### `tlb_dynamic_lb`：TLB 动态负载均衡

在 `balance-tlb` 模式下启用动态负载均衡。当设置为 `true` 时，Bond 会定期
重新平衡流量。

### `ns_ip6_target`：IPv6 邻居请求目标

用作 IPv6 链路监控的邻居请求目标的 IPv6 地址列表。
仅在 `arp_interval` 大于 0 时有效。

## `ports`：Bond 端口

Bond 的端口配置列表。应用时，如果定义了，将覆盖当前的端口列表。

每个端口条目支持：

### `name`：端口名称

Bond 端口的接口名称。必填。

### `priority`：端口优先级

故障切换的端口优先级。仅在 `active-backup`、`balance-tlb` 和 `balance-alb`
模式下有效。

### `queue-id`：端口队列 ID

分配给此端口的队列 ID。Linux 内核不支持多个端口共享相同的队列 ID。
