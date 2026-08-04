# 无守护进程模式

当在 `npt apply <YAML_FILE>` 中传入 `--no-daemon` 选项时，
`npt` 命令不会联系 nipart 守护进程，而是直接将配置应用到内核或相关守护进程
（例如 OpenvSwitch 守护进程）。

限制：
 * 无法续订 DHCP 租约。当你在期望的 YAML 文件中请求 DHCP 时，
   `npt` 会请求 DHCP 租约并通过 `preferred-life-time` 和
   `valid-life-time` 将 DHCP 租约的 IP 地址应用到内核，使内核在租约过期后
   清除该 IP。因此你需要定期运行 `np apply <YAML_FILE> --no-daemon` 来续订
   DHCP 租约。

 * 不支持[条件化接口启停](./auto_connect.md)，因为没有守护进程来监控链路载体状态
   并有条件地触发接口启停。请使用守护进程模式代替。
