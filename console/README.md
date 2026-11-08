# Console

纯字符 world 控制台，不创建窗口，不领取 Human，也不发送移动输入。

```sh
cargo run -p world
cargo run -p console
```

启动后处于离线状态。输入以下命令连接并检查人物：

```text
connect
peek 1
disconnect
connect alice@192.168.1.10:5000
peek 2
exit
```

## 命令

| 命令 | 行为 |
|---|---|
| `connect [用户名][@IP[:PORT]]` | 连接 world，默认 `spoon@127.0.0.1:5000` |
| `peek GeneralName值` | 显示该名字对应的所有 Human 的同步组件快照 |
| `disconnect` | 断开连接并清除旧快照 |
| `help` | 显示语法和示例 |
| `exit` | 主动断开并退出；输入结束 EOF 也会退出 |

用户名、IP、端口可分别省略：`connect`、`connect alice`、
`connect @192.168.1.10`、`connect alice@192.168.1.10`、`connect alice@:5000`。
IPv6 使用方括号，例如 `connect spoon@[::1]:5000`。地址仅接受 IP，不解析域名。
用户名默认 `spoon`，允许 1–64 个 UTF-8 字节，不含空白、控制字符或 `@`。
相同用户名可以建立多个独立会话；用户名是会话标识，本次不提供密码认证。

`connect` 最多等待 10 秒，收到世界数据后返回。连接失败后可以重试；
已有连接时须先执行 `disconnect`，不会自动重连。

## peek 输出

`GeneralName` 目前是 `usize`，所以使用 `peek 1`，而非字面上的 `peek GeneralName`。
输出所有匹配人物，按 `HumanId` 排序，每个人物的组件按名字排序。
组件通过反射展开，包括私有字段、枚举和 `Bounded` 的 `value`、`min`、`max`。
当前包括 `GeneralName`、`GeneralKind`、`Breathing`、`Movement`、`PersonPosition`、
`HumanId`、`HumanStatus`。新增同步组件时请在共享插件中注册反射，
否则显示组件名及无法展开的提示。

读取的是 console 持续接收的最新本地同步快照，不包含 world 私有的
`Owner`、`MoveIntent`，也不包含客户端内部组件。等待输入时同步仍会继续；
断线后不允许读取旧数据。后台状态消息在下一次命令或提示符刷新时输出，避免打断输入。

终端支持 ANSI 且 `TERM` 不是 `dumb` 时可输出彩色文字（Windows 自动尝试启用）；设置 `NO_COLOR`
或重定向输出时使用纯文本。普通标准输入不提供补全、历史记录或 shell 执行。

## 其他客户端

所有连接都携带用户名与客户端类型。observer 默认为 `spoon`，可使用：

```sh
cargo run -p observer -- --username alice
```

touchpad 使用 URL 参数，例如 `https://localhost:8080/?username=alice`。
只有 observer 会领取 Human，console 与 touchpad 均为只读会话。
协议版本为 3，world 与所有客户端需要一同重建。
