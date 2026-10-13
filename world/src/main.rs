//! `world` 是服务端（headless 权威模拟进程）：
//!
//! 1. `MinimalPlugins` 启动一个无窗口的 Bevy 应用（手动补上 `LogPlugin` 和 `StatesPlugin`）。
//! 2. 启动时创建 `Person` 实体并标记复制（`Replicated` 由 required component 自动添加）。
//! 3. 客户端连上时**分配**一个空闲 Person 给它（`Owner`），断开时释放。
//! 4. 接收 observer 发来的 `MoveDirection`，只写入该客户端所有实体的 `MoveIntent`（输入缓冲）。
//! 5. `FixedUpdate` 里统一按固定时间步积分位置，再由 bevy_replicon 自动同步。
//!
//! 运行：`cargo run -p world`

use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    path::PathBuf,
    time::SystemTime,
};

use base::{
    BasePlugin, HumanId, HumanStatus, MoveDirection, PORT, PROTOCOL_ID, Person, PersonPosition,
    TouchpadConnection, WorldOverview, log_plugin,
};
use bevy::{prelude::*, state::app::StatesPlugin};
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use bevy_replicon_renet2::{
    RenetChannelsExt, RepliconRenetPlugins,
    netcode::{
        BoxedSocket, NativeSocket, NetcodeServerTransport, ServerAuthentication, ServerSetupConfig,
        WebTransportServer, WebTransportServerConfig,
    },
    renet2::{ConnectionConfig, RenetServer},
};

/// 归属组件：这个 Person 由哪个客户端控制。
///
/// 刻意**不**放在 base、也**不**注册 `replicate`：
/// - replicon 0.41 的 `ClientId` 不可序列化，无法走网络；
/// - 归属裁决保持服务端私有；监控使用 HumanStatus.owner 的网络 ID 摘要。
#[derive(Component, Debug)]
pub struct Owner(pub ClientId);

/// 输入缓冲：该实体**当前**的移动意图（单位方向向量）。
///
/// 同样是服务端私有组件。网络消息只负责刷新它，
/// 真正的位移积分统一放在 `integrate_movement`（FixedUpdate）里做，
/// 避免了"一帧收到多条消息就重复乘 delta"的问题。
#[derive(Component, Deref, DerefMut, Debug, Default)]
pub struct MoveIntent(pub Vec2);

#[derive(Resource)]
struct ServerOptions {
    webtransport: bool,
    bind: IpAddr,
    advertise_ip: Option<IpAddr>,
    webtransport_port: u16,
    touchpad_config: PathBuf,
}

impl ServerOptions {
    fn parse() -> std::result::Result<Option<Self>, Box<dyn std::error::Error>> {
        let mut options = Self {
            webtransport: false,
            bind: Ipv4Addr::LOCALHOST.into(),
            advertise_ip: None,
            webtransport_port: 5001,
            touchpad_config: "touchpad/connection.json".into(),
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--help" | "-h" => {
                    println!(
                        "world [--webtransport --advertise-ip IP] [--bind IP] [--webtransport-port PORT] [--touchpad-config PATH]"
                    );
                    return Ok(None);
                }
                "--webtransport" => options.webtransport = true,
                "--bind" | "--advertise-ip" | "--webtransport-port" | "--touchpad-config" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("missing value for {argument}"))?;
                    match argument.as_str() {
                        "--bind" => options.bind = value.parse()?,
                        "--advertise-ip" => options.advertise_ip = Some(value.parse()?),
                        "--webtransport-port" => options.webtransport_port = value.parse()?,
                        _ => options.touchpad_config = value.into(),
                    }
                }
                _ => return Err(format!("unknown argument: {argument}").into()),
            }
        }
        if options.webtransport && options.advertise_ip.is_none() {
            return Err(
                "--webtransport requires --advertise-ip (use 127.0.0.1 for local development)"
                    .into(),
            );
        }
        if let Some(ip) = options.advertise_ip {
            if ip.is_unspecified() || ip.is_multicast() {
                return Err("--advertise-ip must be a reachable unicast IP".into());
            }
            if !options.bind.is_unspecified() && options.bind != ip {
                return Err("--bind must match --advertise-ip or be an unspecified address".into());
            }
        }
        if options.webtransport
            && (options.webtransport_port == PORT || options.webtransport_port == 0)
        {
            return Err(
                "WebTransport needs a nonzero port different from the UDP player port".into(),
            );
        }
        Ok(Some(options))
    }
}

#[derive(Resource)]
struct NetworkRuntime {
    _runtime: tokio::runtime::Runtime,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ConnectionRole {
    Player,
    Dashboard,
}

fn main() {
    let options = match ServerOptions::parse() {
        Ok(Some(options)) => options,
        Ok(None) => return,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    App::new()
        .insert_resource(options)
        .add_plugins((
            // MinimalPlugins 不含 LogPlugin 和 StatesPlugin，需要手动补齐
            // （日志配置由 base::log_plugin() 统一下发；replicon 依赖 States）。
            MinimalPlugins,
            StatesPlugin,
            log_plugin(),
            // replicon 核心 + renet2 传输后端，顺序：先核心，后后端，最后共享注册。
            RepliconPlugins,
            RepliconRenetPlugins,
            BasePlugin,
        ))
        // bevy 0.19 的 fallible system（返回 Result 的系统）不能放进元组，
        // 必须像这样单独注册；出错时默认 panic 并打印错误。
        .add_systems(Startup, start_server)
        .add_systems(Startup, spawn_people)
        .add_systems(OnEnter(ServerState::Running), || info!("server is running"))
        .add_observer(client_connected)
        .add_observer(client_disconnected)
        // 消息到达：只把输入写进缓冲，不做位移。
        .add_observer(buffer_input)
        // 权威移动：固定时间步，每 tick 只积分一次。
        .add_systems(FixedUpdate, integrate_movement)
        .add_systems(PostUpdate, update_overview.before(ServerSystems::Send))
        .run();
}

/// 启动 renet2/netcode 服务端，并把 `RenetServer` / `NetcodeServerTransport`
/// 作为资源插入。bevy_renet2 的插件检测到这两个资源后自动开始收发。
///
/// 必须在所有通道注册完成之后（即 `BasePlugin` 之后）读取 `RepliconChannels`，
/// 因此放在 Startup 而不是 main 里。
fn start_server(
    mut commands: Commands,
    channels: Res<RepliconChannels>,
    options: Res<ServerOptions>,
) -> Result<()> {
    info!("starting server at port {PORT}");

    // 用注册好的 replicon 通道生成 renet 连接配置。
    let server = RenetServer::new(ConnectionConfig::from_channels(
        channels.server_configs(),
        channels.client_configs(),
    ));

    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let public_addr = SocketAddr::new(options.advertise_ip.unwrap_or(options.bind), PORT);
    let socket = UdpSocket::bind(SocketAddr::new(options.bind, PORT))?;
    let mut sockets = vec![BoxedSocket::new(NativeSocket::new(socket)?)];
    let mut socket_addresses = vec![vec![public_addr]];
    let mut bootstrap = None;
    if options.webtransport {
        let advertised = SocketAddr::new(options.advertise_ip.unwrap(), options.webtransport_port);
        let listen = SocketAddr::new(options.bind, options.webtransport_port);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let (config, hash) = WebTransportServerConfig::new_selfsigned_with_proxies(
            listen,
            vec![advertised.into()],
            10,
        )?;
        let socket = WebTransportServer::new(config, runtime.handle().clone())?;
        sockets.push(BoxedSocket::new(socket));
        socket_addresses.push(vec![advertised]);
        commands.insert_resource(NetworkRuntime { _runtime: runtime });
        bootstrap = Some(TouchpadConnection {
            address: advertised.to_string(),
            certificate_hash: hash.hash,
            socket_index: 1,
            protocol_version: PROTOCOL_ID,
        });
        info!("WebTransport listening on {listen}, advertised as {advertised}");
    }
    let server_config = ServerSetupConfig {
        current_time,
        max_clients: 10,
        protocol_id: PROTOCOL_ID,
        authentication: ServerAuthentication::Unsecure,
        socket_addresses,
    };
    let transport = NetcodeServerTransport::new_with_sockets(server_config, sockets)?;

    if let Some(bootstrap) = bootstrap {
        let parent = options
            .touchpad_config
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(&bootstrap)?)?;
        file.as_file().sync_all()?;
        file.persist(&options.touchpad_config)?;
        info!("wrote {}", options.touchpad_config.display());
    }

    commands.insert_resource(server);
    commands.insert_resource(transport);
    Ok(())
}

/// 生成 3 个 Person 实体（此时无人认领）。
///
/// 只写 `Person` + `PersonPosition`，`Replicated` 由 `#[require(...)]` 自动补上。
fn spawn_people(mut commands: Commands) {
    const STARTS: [(&str, Vec2); 3] = [
        ("Alice", Vec2::new(-120.0, 0.0)),
        ("Bob", Vec2::new(0.0, 0.0)),
        ("Carol", Vec2::new(120.0, 0.0)),
    ];

    for (index, (name, position)) in STARTS.into_iter().enumerate() {
        commands.spawn((
            Person { name: name.into() },
            PersonPosition(position),
            HumanId(index as u64 + 1),
            HumanStatus::default(),
        ));
        info!("spawned person `{name}` at {position:?}");
    }
    commands.spawn((WorldOverview::default(), Replicated));
}

/// 客户端接入：replicon 把每个已连接客户端表示为一个实体（`ConnectedClient`），
/// 我们把一个**无主**的 Person 分配给它（`Owner` + 清零的 `MoveIntent`）。
///
/// `ClientId::Client(entity)` 里的 entity 就是这个客户端实体——
/// 之后 `FromClient` 消息里的 `trigger.client_id` 也是同一个值，可以直接比较。
fn client_connected(
    trigger: On<Add, ConnectedClient>,
    mut commands: Commands,
    free_people: Query<(Entity, &Person), Without<Owner>>,
    clients: Query<&NetworkId>,
    transport: Res<NetcodeServerTransport>,
) {
    let client_id = ClientId::Client(trigger.entity);
    info!("client `{client_id}` connected");

    let Ok(network_id) = clients.get(trigger.entity) else {
        return;
    };
    let Some((socket_index, _)) = transport.client_addr(network_id.get()) else {
        return;
    };
    let role = if socket_index == 0 {
        ConnectionRole::Player
    } else {
        ConnectionRole::Dashboard
    };
    commands.entity(trigger.entity).insert(role);
    if role == ConnectionRole::Dashboard {
        info!("dashboard `{client_id}` connected without claiming a human");
        return;
    }

    if let Some((entity, person)) = free_people.iter().next() {
        commands
            .entity(entity)
            .insert((Owner(client_id), MoveIntent::default()));
        info!("assigned `{}` to `{client_id}`", person.name);
    } else {
        // Person 只有 3 个，第 4 个客户端只能旁观（真实项目应按需生成）。
        warn!("no free person for `{client_id}`, it can only observe");
    }
}

/// 客户端断开：释放它占用的 Person（摘掉 `Owner` 和 `MoveIntent`），
/// 下一个连入的客户端可以复用。
fn client_disconnected(
    trigger: On<Remove, ConnectedClient>,
    mut commands: Commands,
    people: Query<(Entity, &Owner, &Person)>,
) {
    let client_id = ClientId::Client(trigger.entity);
    info!("client `{client_id}` disconnected");

    if let Some((entity, _, person)) = people.iter().find(|(_, owner, _)| owner.0 == client_id) {
        commands.entity(entity).remove::<(Owner, MoveIntent)>();
        info!("released `{}` from `{client_id}`", person.name);
    }
}

/// 收到客户端输入：只刷新发送者自己实体的 `MoveIntent`，不做任何位移。
///
/// 这就是"归属"落地的地方——`trigger.client_id` 和 `Owner` 对得上才生效，
/// 其他客户端的 Person 完全不受影响（想作弊替别人移动？门都没有）。
///
/// 客户端每帧都会上报（含零向量），所以这里用 debug 级别防刷屏。
fn buffer_input(
    trigger: On<FromClient<MoveDirection>>,
    mut people: Query<(&Owner, &mut MoveIntent)>,
    clients: Query<&ConnectionRole>,
) {
    let ClientId::Client(client) = trigger.client_id else {
        return;
    };
    if clients.get(client) != Ok(&ConnectionRole::Player) {
        return;
    }
    debug!(
        "received `{}` from `{}`",
        *trigger.message, trigger.client_id
    );

    if let Some((_, mut intent)) = people
        .iter_mut()
        .find(|(owner, _)| owner.0 == trigger.client_id)
    {
        **intent = *trigger.message;
    } else {
        debug!("`{}` owns no person, input ignored", trigger.client_id);
    }
}

/// 权威移动：固定时间步积分，每 tick 恰好一次，与消息到达频率无关。
///
/// `intent` 为零时**跳过写入**： ECS 中"写"会打改变检测标记，
/// 即使值没变也会让 replicon 认为组件变了而发包——
/// 空闲时少写 = 空闲时零流量。
fn integrate_movement(
    time: Res<Time<Fixed>>,
    mut people: Query<(&mut PersonPosition, Option<&MoveIntent>, &mut HumanStatus)>,
) {
    const MOVE_SPEED: f32 = 300.0;

    for (mut position, intent, mut status) in &mut people {
        let mut moving = false;
        if let Some(intent) = intent.filter(|intent| intent.0 != Vec2::ZERO) {
            let previous = position.0;
            position.0 += intent.0 * time.delta_secs() * MOVE_SPEED;
            moving = position.0 != previous;
        }
        if status.moving != moving {
            status.moving = moving;
        }
    }
}

fn update_overview(
    clients: Query<(&NetworkId, &ConnectionRole), With<ConnectedClient>>,
    mut overview: Query<&mut WorldOverview>,
    mut people: Query<(Option<&Owner>, &mut HumanStatus)>,
    network_ids: Query<&NetworkId>,
) {
    let mut current = WorldOverview::default();
    for (_, role) in &clients {
        current.total_connections += 1;
        match role {
            ConnectionRole::Player => current.player_connections += 1,
            ConnectionRole::Dashboard => current.dashboard_connections += 1,
        }
    }
    for mut overview in &mut overview {
        overview.set_if_neq(current);
    }
    for (owner, mut status) in &mut people {
        let owner_id = owner.and_then(|owner| match owner.0 {
            ClientId::Client(entity) => network_ids.get(entity).ok().map(|id| id.get()),
            _ => None,
        });
        if status.owner != owner_id {
            status.owner = owner_id;
        }
    }
}
