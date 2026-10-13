//! `observer` 是客户端（带窗口的观察进程）：
//!
//! 1. `DefaultPlugins`（仅启用 bevy 的 `2d` feature）提供窗口、渲染、输入和日志。
//! 2. 连接 world 后，replicon 自动镜像服务端的 `Person` / `PersonPosition` 组件。
//! 3. 每帧读取 WASD 按键 -> `client_trigger(MoveDirection)` 发给服务端。
//! 4. 用 Gizmos 把每个 Person 画成彩色方块（纯表现层，不给实体加渲染组件）。
//!
//! 运行：`cargo run -p observer`（先启动 world）

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::SystemTime,
};

use base::{BasePlugin, MoveDirection, PORT, PROTOCOL_ID, Person, PersonPosition, log_plugin};
use bevy::{
    color::palettes::css,
    prelude::*,
    winit::{UpdateMode::Continuous, WinitSettings},
};
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::{
    RenetChannelsExt, RepliconRenetPlugins,
    netcode::{ClientAuthentication, NativeSocket, NetcodeClientTransport},
    renet2::{ConnectionConfig, RenetClient},
};

/// 与生成顺序一一对应的展示颜色（纯客户端表现，不参与网络同步）。
const COLORS: [Srgba; 3] = [css::GREEN, css::RED, css::BLUE];

/// 方块大小（逻辑像素）。
const SIZE: f32 = 50.0;

fn main() {
    App::new()
        .add_plugins((
            // DefaultPlugins 已含 LogPlugin，用 .set() 把日志配置换成 base 提供的版本
            // （窗口 + 渲染 + 输入 + LogPlugin + StatesPlugin 都在 DefaultPlugins 内）。
            DefaultPlugins.set(log_plugin()),
            RepliconPlugins,
            RepliconRenetPlugins,
            BasePlugin,
        ))
        // 失焦时也保持满帧率，方便观察网络同步。
        .insert_resource(WinitSettings {
            focused_mode: Continuous,
            unfocused_mode: Continuous,
        })
        // fallible system 不能放进元组，单独注册（出错时默认 panic）。
        .add_systems(Startup, connect_server)
        .add_systems(Startup, spawn_camera)
        .add_systems(OnEnter(ClientState::Connected), || {
            info!("connected to server")
        })
        .add_observer(person_arrived)
        .add_observer(person_removed)
        .add_systems(
            Update,
            (send_input, draw_people, log_positions).run_if(in_state(ClientState::Connected)),
        )
        .run();
}

/// 启动 renet2/netcode 客户端并连接服务端（127.0.0.1:5000）。
///
/// 与服务端同理：必须在 `BasePlugin` 注册完通道之后读取 `RepliconChannels`。
fn connect_server(mut commands: Commands, channels: Res<RepliconChannels>) -> Result<()> {
    let ip = Ipv4Addr::LOCALHOST;
    info!("connecting to {ip}:{PORT}");

    // 第二个参数 false：UDP 是不可靠底层通道（WebSocket/内存通道才传 true）。
    let client = RenetClient::new(
        ConnectionConfig::from_channels(channels.server_configs(), channels.client_configs()),
        false,
    );

    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let client_id = current_time.as_millis() as u64; // 用时间戳当临时客户端 ID
    let server_addr = SocketAddr::new(IpAddr::V4(ip), PORT);
    let socket = UdpSocket::bind((ip, 0))?; // 本地随机端口
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        socket_id: 0,
        server_addr,
        user_data: None,
    };
    let transport =
        NetcodeClientTransport::new(current_time, authentication, NativeSocket::new(socket)?)?;

    commands.insert_resource(client);
    commands.insert_resource(transport);
    Ok(())
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// 复制到达：服务端生成 Person 时，客户端世界自动出现镜像实体。
///
/// 注意 `trigger.entity` 是**本地**实体 ID——服务端和客户端的实体 ID 不同，
/// replicon 内部通过 `ServerEntityMap` 维护映射。
fn person_arrived(trigger: On<Add, Person>, people: Query<&Person>) {
    if let Ok(person) = people.get(trigger.entity) {
        info!(
            "person `{}` arrived as local entity {}",
            person.name, trigger.entity
        );
    }
}

/// 服务端删除（或断开清理）实体时，镜像实体也会被移除。
fn person_removed(trigger: On<Remove, Person>) {
    info!("person entity {} removed", trigger.entity);
}

/// 读取键盘输入并发送给服务端。
///
/// 客户端**不移动**任何实体，只上报意图（输入方向），
/// 移动完全由服务端计算后再同步回来——这就是服务端权威模型。
///
/// 每帧都发（包括零向量）：消息语义是“这是我**当前**的输入状态”，
/// 而不是“请按这个方向动一下”。松开按键也要发 ZERO，
/// 否则服务端的 `MoveIntent` 会一直保持旧方向，方块停不下来。
/// 真实项目通常按固定频率（如 20Hz）采样发送以节省带宽。
fn send_input(mut commands: Commands, input: Res<ButtonInput<KeyCode>>) {
    let mut direction = Vec2::ZERO;
    if input.pressed(KeyCode::KeyW) {
        direction.y += 1.0;
    }
    if input.pressed(KeyCode::KeyS) {
        direction.y -= 1.0;
    }
    if input.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if input.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }

    commands.client_trigger(MoveDirection(direction.normalize_or_zero()));
}

/// 表现层：把同步过来的纯数据画成方块。
///
/// Gizmos 只需要一个相机即可绘制，不需要给实体插入任何渲染组件，
/// 这样网络数据（base）与表现（observer）完全解耦。
fn draw_people(mut gizmos: Gizmos, people: Query<&PersonPosition>) {
    for (index, position) in people.iter().enumerate() {
        let color: Color = COLORS[index % COLORS.len()].into();
        gizmos.rect_2d(position.0, Vec2::splat(SIZE), color);
    }
}

/// 每帧打印同步到的位置（debug 级别，默认不显示；
/// 想观察同步效果可设置环境变量 `RUST_LOG=debug` 再运行）。
fn log_positions(people: Query<(&Person, &PersonPosition), Changed<PersonPosition>>) {
    for (person, position) in &people {
        debug!("`{}` is now at {:?}", person.name, position.0);
    }
}
