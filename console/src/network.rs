use std::{
    net::{Ipv4Addr, Ipv6Addr, UdpSocket},
    sync::{Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};

use base::{BasePlugin, HumanId, PROTOCOL_ID, WorldOverview, general::GeneralName};
use bevy::{app::ScheduleRunnerPlugin, ecs::reflect::ReflectComponent, prelude::*};
use bevy_replicon::prelude::*;
use bevy_replicon::shared::replication::rules::ReplicationRules;
use bevy_replicon_renet2::{
    RenetChannelsExt, RepliconRenetPlugins,
    netcode::{
        ClientAuthentication, NativeSocket, NetcodeClientTransport, NetcodeTransportError,
        generate_random_bytes,
    },
    renet2::{ConnectionConfig, RenetClient},
};

use crate::command::{Command, ConnectionTarget, HELP};

pub enum Request {
    Execute(Command, mpsc::Sender<Reply>),
    Status(mpsc::Sender<Reply>),
}

pub struct Reply {
    pub output: Result<String, String>,
    pub connection: Option<String>,
}

#[derive(Resource)]
struct Inbox(Mutex<mpsc::Receiver<Request>>);

struct PendingConnection {
    started: Instant,
    reply: mpsc::Sender<Reply>,
}

#[derive(Resource, Default)]
struct Session {
    target: Option<ConnectionTarget>,
    pending: Option<PendingConnection>,
    notice: Option<String>,
    transport_error: Option<String>,
}

pub fn run(receiver: mpsc::Receiver<Request>) {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
            bevy::state::app::StatesPlugin,
            RepliconPlugins,
            RepliconRenetPlugins,
            BasePlugin,
        ))
        .insert_resource(Inbox(Mutex::new(receiver)))
        .init_resource::<Session>()
        .add_systems(Update, (capture_transport_errors, update_console).chain())
        .run();
}

fn capture_transport_errors(
    mut errors: MessageReader<NetcodeTransportError>,
    mut session: ResMut<Session>,
) {
    for error in errors.read() {
        if let NetcodeTransportError::IO(error) = error {
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) {
                continue;
            }
        }
        if session.target.is_some() {
            session.transport_error = Some(error.to_string());
        }
    }
}

fn update_console(world: &mut World) {
    update_connection(world);
    let request = world.resource::<Inbox>().0.lock().unwrap().try_recv();
    match request {
        Ok(Request::Status(sender)) => {
            let notice = world.resource_mut::<Session>().notice.take();
            respond(world, sender, Ok(notice.unwrap_or_default()));
        }
        Ok(Request::Execute(command, sender)) => match command {
            Command::Connect(target) => {
                if world.resource::<Session>().target.is_some() {
                    respond(
                        world,
                        sender,
                        Err("Already connected. Run disconnect first.".into()),
                    );
                    return;
                }
                match make_connection(&target, world.resource::<RepliconChannels>()) {
                    Ok((client, transport)) => {
                        world.insert_resource(client);
                        world.insert_resource(transport);
                        let mut session = world.resource_mut::<Session>();
                        session.target = Some(target);
                        session.pending = Some(PendingConnection {
                            started: Instant::now(),
                            reply: sender,
                        });
                    }
                    Err(error) => respond(world, sender, Err(error)),
                }
            }
            Command::Peek(name) => {
                let output = if world.resource::<Session>().target.is_none() {
                    Err("Not connected. Run connect first.".into())
                } else {
                    peek(world, name)
                };
                respond(world, sender, output);
            }
            Command::Disconnect => {
                close_connection(world);
                respond(world, sender, Ok("Disconnected.".into()));
            }
            Command::Help => respond(world, sender, Ok(HELP.into())),
            Command::Exit => {
                close_connection(world);
                respond(world, sender, Ok("Bye.".into()));
                world.write_message(AppExit::Success);
            }
        },
        Err(mpsc::TryRecvError::Disconnected) => {
            close_connection(world);
            world.write_message(AppExit::Success);
        }
        Err(mpsc::TryRecvError::Empty) => {}
    }
}

fn respond(world: &World, sender: mpsc::Sender<Reply>, output: Result<String, String>) {
    let _ = sender.send(Reply {
        output,
        connection: world
            .resource::<Session>()
            .target
            .as_ref()
            .map(ToString::to_string),
    });
}

fn make_connection(
    target: &ConnectionTarget,
    channels: &RepliconChannels,
) -> Result<(RenetClient, NetcodeClientTransport), String> {
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    let socket = if target.address.is_ipv4() {
        UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
    } else {
        UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0))
    }
    .map_err(|error| error.to_string())?;
    let authentication = ClientAuthentication::Unsecure {
        client_id: u64::from_le_bytes(generate_random_bytes()),
        protocol_id: PROTOCOL_ID,
        socket_id: 0,
        server_addr: target.address,
        user_data: Some(target.identity.encode()),
    };
    let socket = NativeSocket::new(socket).map_err(|error| error.to_string())?;
    let transport = NetcodeClientTransport::new(current_time, authentication, socket)
        .map_err(|error| error.to_string())?;
    let client = RenetClient::new(
        ConnectionConfig::from_channels(channels.server_configs(), channels.client_configs()),
        false,
    );
    Ok((client, transport))
}

fn update_connection(world: &mut World) {
    if world.resource::<Session>().target.is_none() {
        return;
    }
    let transport_error = world.resource_mut::<Session>().transport_error.take();
    let disconnected = transport_error.is_some()
        || world
            .get_resource::<RenetClient>()
            .is_none_or(|client| client.is_disconnected())
        || world
            .get_resource::<NetcodeClientTransport>()
            .is_none_or(|transport| {
                transport.is_disconnected()
                    || transport.time_since_last_received_packet() >= Duration::from_secs(10)
            });
    let timed_out = world
        .resource::<Session>()
        .pending
        .as_ref()
        .is_some_and(|pending| pending.started.elapsed() >= Duration::from_secs(10));
    if disconnected || timed_out {
        let reason = if let Some(error) = transport_error {
            format!("Disconnected: {error}")
        } else if timed_out {
            "Connection timed out after 10 seconds.".into()
        } else {
            let detail = world
                .get_resource::<NetcodeClientTransport>()
                .and_then(|transport| transport.disconnect_reason())
                .map(|reason| format!("{reason:?}"))
                .or_else(|| {
                    world
                        .get_resource::<RenetClient>()
                        .and_then(|client| client.disconnect_reason())
                        .map(|reason| format!("{reason:?}"))
                })
                .unwrap_or_else(|| "Connection closed".into());
            format!("Disconnected: {detail}")
        };
        let pending = world.resource_mut::<Session>().pending.take();
        close_connection(world);
        if let Some(pending) = pending {
            respond(world, pending.reply, Err(reason));
        } else {
            world.resource_mut::<Session>().notice = Some(reason);
        }
        return;
    }
    let ready = *world.resource::<State<ClientState>>().get() == ClientState::Connected
        && world.query::<&WorldOverview>().iter(world).next().is_some();
    if ready {
        let pending = world.resource_mut::<Session>().pending.take();
        if let Some(pending) = pending {
            let target = world.resource::<Session>().target.as_ref().unwrap();
            respond(
                world,
                pending.reply,
                Ok(format!("Connected to {target}. World synchronized.")),
            );
        }
    }
}

fn close_connection(world: &mut World) {
    if let Some(mut transport) = world.get_resource_mut::<NetcodeClientTransport>() {
        transport.disconnect();
    }
    world.remove_resource::<NetcodeClientTransport>();
    world.remove_resource::<RenetClient>();
    world
        .resource_mut::<NextState<ClientState>>()
        .set(ClientState::Disconnected);
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Remote>>()
        .iter(world)
        .collect();
    for entity in entities {
        world.despawn(entity);
    }
    world.resource_mut::<Session>().target = None;
    world.resource_mut::<Session>().transport_error = None;
}

fn peek(world: &mut World, name: usize) -> Result<String, String> {
    let mut humans: Vec<_> = world
        .query::<(Entity, &GeneralName, &HumanId)>()
        .iter(world)
        .filter(|(_, general_name, _)| general_name.0 == name)
        .map(|(entity, _, id)| (id.0, entity))
        .collect();
    humans.sort_by_key(|(id, entity)| (*id, *entity));
    if humans.is_empty() {
        return Err(format!("No Human with GeneralName {name}."));
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let rules = world.resource::<ReplicationRules>();
    let mut output = String::new();
    for (id, entity) in humans {
        output.push_str(&format!("Human {id} (GeneralName {name})\n"));
        let entity = world.entity(entity);
        let mut components = Vec::new();
        for component_id in entity.archetype().iter_components() {
            if !rules.iter().any(|rule| {
                rule.components
                    .iter()
                    .any(|component| component.id == component_id)
            }) {
                continue;
            }
            let Some(info) = world.components().get_info(component_id) else {
                continue;
            };
            let full_name = info.name().to_string();
            let component_name = full_name
                .rsplit("::")
                .next()
                .unwrap_or(&full_name)
                .to_string();
            let reflected = info
                .type_id()
                .and_then(|type_id| registry.get(type_id))
                .and_then(|registration| registration.data::<ReflectComponent>())
                .and_then(|component| component.reflect(entity));
            let value = reflected.map_or_else(
                || "<cannot expand: reflection is not registered>".into(),
                |value| format!("{value:#?}"),
            );
            components.push((component_name, value));
        }
        components.sort_by(|left, right| left.0.cmp(&right.0));
        for (component_name, value) in components {
            output.push_str(&format!("  {component_name}:\n"));
            for line in value.lines() {
                output.push_str(&format!("    {line}\n"));
            }
        }
    }
    Ok(output.trim_end().into())
}
