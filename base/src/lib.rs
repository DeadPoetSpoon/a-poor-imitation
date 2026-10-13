//! `base` 是共享 crate：服务端（world）和客户端（observer）**共用**的类型与注册逻辑。
//!
//! bevy_replicon 要求两侧的组件 / 消息**注册顺序完全一致**，
//! 因此所有涉及网络的注册都集中在本 crate 的 [`BasePlugin`] 中，
//! 由 world 和 observer 各自调用一次，避免两边手写导致协议不匹配。
//!
//! 本 crate 只定义"是什么"（数据与协议），不包含任何游戏逻辑或渲染逻辑。

use bevy::{
    log::{DEFAULT_FILTER, Level, LogPlugin},
    prelude::*,
};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

/// 协议版本号。客户端与服务端不一致时 netcode 会拒绝连接。
/// 修改网络类型（增删组件/消息）后应当变更它。
pub const PROTOCOL_ID: u64 = 1;

/// 服务端监听端口（仅本机教学示例用）。
pub const PORT: u16 = 5000;

/// 日志级别：
///
/// - 开启 `dev` 功能时用 `DEBUG`（开发期看同步细节与系统调度）；
/// - 否则用 `INFO`（`rls` 场景即此档）。
///
/// 两者同时开启时 `dev` 优先。
pub const LOG_LEVEL: Level = if cfg!(feature = "dev") {
    Level::DEBUG
} else {
    Level::INFO
};

/// 统一的日志插件配置，由 `base` 提供给两个进程，避免两边各写一份。
///
/// - world（headless）：`MinimalPlugins` 不含 LogPlugin，直接加进插件元组；
/// - observer：`DefaultPlugins` **已包含** LogPlugin，需用
///   `DefaultPlugins.set(log_plugin())` 覆盖它，不能再单独 add 一个。
///
/// `filter` 保留 bevy 的默认降噪规则（wgpu/naga 等只报错误），
/// 真正的详细程度由 `level` 控制。
///
/// 注意：`RUST_LOG` 环境变量会覆盖这里的设置，运行时调试优先用它。
pub fn log_plugin() -> LogPlugin {
    LogPlugin {
        level: LOG_LEVEL,
        filter: DEFAULT_FILTER.to_string(),
        ..Default::default()
    }
}

/// 统一注册所有需要复制（replicate）的组件和需要传输的远程事件。
///
/// 必须在 `RepliconPlugins` 之后添加。
pub struct BasePlugin;

impl Plugin for BasePlugin {
    fn build(&self, app: &mut App) {
        app // 注册顺序两侧必须一致！
            .replicate::<Person>() // 复制规则：实体上的 Person 组件
            .replicate::<PersonPosition>() // 复制规则：实体上的 PersonPosition 组件
            .replicate::<HumanId>()
            .replicate::<HumanStatus>()
            .replicate::<WorldOverview>()
            .add_client_event::<MoveDirection>(Channel::Ordered); // 客户端 -> 服务端消息
    }
}

/// "人"实体：低频、几乎不变的数据放在这里。
///
/// - `Serialize + Deserialize`：复制时通过 postcard 序列化。
/// - `require(PersonPosition)`：通过 Bevy 的 required components，
///   生成 `Person` 时自动补上位置组件。
/// - `require(Replicated)`：自动打上 replicon 的复制标记，
///   该实体（的已注册组件）会自动同步给客户端。
#[derive(Component, Serialize, Deserialize, Debug, Default)]
#[require(PersonPosition, Replicated)]
pub struct Person {
    /// 名字：只在生成时写入，之后基本不变。
    pub name: String,
}

/// "人"的位置：高频变化的数据单独拆成一个组件。
///
/// 拆分的原因：replicon 按"组件"为单位做增量复制，
/// 位置每帧都变、名字几乎不变。拆开后同步的数据量更小。
#[derive(Component, Deref, DerefMut, Serialize, Deserialize, Debug, Default)]
pub struct PersonPosition(pub Vec2);

/// 客户端 -> 服务端的移动输入。
///
/// `Event`：通过 `commands.client_trigger(...)` 发送，
/// 服务端以 `FromClient<MoveDirection>` 的形式收到（含发送者身份）。
/// `Channel::Ordered`：可靠有序，保证输入不丢、不乱序。
#[derive(Event, Serialize, Deserialize, Debug, Deref)]
pub struct MoveDirection(pub Vec2);

#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanId(pub u64);

#[derive(Component, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HumanStatus {
    pub moving: bool,
    pub owner: Option<u64>,
}

#[derive(Component, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorldOverview {
    pub total_connections: u32,
    pub player_connections: u32,
    pub dashboard_connections: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TouchpadConnection {
    pub address: String,
    pub certificate_hash: [u8; 32],
    pub socket_index: u8,
    pub protocol_version: u64,
}
