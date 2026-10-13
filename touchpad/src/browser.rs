use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use base::{BasePlugin, PROTOCOL_ID, TouchpadConnection, WorldOverview, log_plugin};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::{
    RenetChannelsExt, RepliconRenetPlugins,
    netcode::{
        ClientAuthentication, NetcodeClientTransport, ServerCertHash, WebTransportClient,
        WebTransportClientConfig, webtransport_is_available_with_cert_hashes,
    },
    renet2::{ConnectionConfig, RenetClient},
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::dashboard::{Dashboard, DashboardSystems, Phase, install};

type FetchResult = (u64, std::result::Result<TouchpadConnection, String>);

#[derive(Resource, Default)]
struct FetchInbox(Arc<Mutex<Vec<FetchResult>>>);

pub fn run() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(log_plugin()).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Touchpad · World monitor".into(),
                canvas: Some("#world-canvas".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }),
        RepliconPlugins,
        RepliconRenetPlugins,
        BasePlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.025, 0.043, 0.065)))
    .init_resource::<FetchInbox>()
    .add_systems(
        Update,
        update_connection.in_set(DashboardSystems::Connection),
    );
    install(&mut app);
    app.run();
}

async fn fetch_configuration() -> std::result::Result<TouchpadConnection, String> {
    let window = web_sys::window().ok_or("No browser window is available")?;
    let options = web_sys::RequestInit::new();
    options.set_cache(web_sys::RequestCache::NoStore);
    let request =
        web_sys::Request::new_with_str_and_init("connection.json", &options).map_err(js_error)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js_error)?
        .dyn_into::<web_sys::Response>()
        .map_err(js_error)?;
    if !response.ok() {
        return Err(format!(
            "Cannot load connection.json (HTTP {}). Start world with --webtransport, then rebuild Trunk.",
            response.status()
        ));
    }
    let text = JsFuture::from(response.text().map_err(js_error)?)
        .await
        .map_err(js_error)?
        .as_string()
        .ok_or("Configuration is not text")?;
    let config: TouchpadConnection =
        serde_json::from_str(&text).map_err(|error| format!("Invalid connection.json: {error}"))?;
    if config.protocol_version != PROTOCOL_ID {
        return Err("Protocol mismatch. Rebuild world, observer, and touchpad together.".into());
    }
    if config.socket_index != 1 {
        return Err("Invalid dashboard socket index; expected 1".into());
    }
    let address: SocketAddr = config
        .address
        .parse()
        .map_err(|_| "Invalid WebTransport address")?;
    if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err("WebTransport address must be a reachable unicast IP and nonzero port".into());
    }
    if config.certificate_hash == [0; 32] {
        return Err("Missing WebTransport certificate hash".into());
    }
    Ok(config)
}

fn js_error(error: JsValue) -> String {
    format!("Browser error: {error:?}")
}

fn make_connection(
    config: &TouchpadConnection,
    channels: &RepliconChannels,
) -> std::result::Result<(RenetClient, NetcodeClientTransport), String> {
    let address: SocketAddr = config
        .address
        .parse()
        .map_err(|_| "Invalid server address")?;
    let socket = WebTransportClient::new(WebTransportClientConfig::new_with_certs(
        address,
        vec![ServerCertHash {
            hash: config.certificate_hash,
        }],
    ));
    let window = web_sys::window().ok_or("No browser window is available")?;
    let mut random = [0; 8];
    window
        .crypto()
        .map_err(js_error)?
        .get_random_values_with_u8_array(&mut random)
        .map_err(js_error)?;
    let authentication = ClientAuthentication::Unsecure {
        client_id: u64::from_le_bytes(random),
        protocol_id: PROTOCOL_ID,
        socket_id: config.socket_index,
        server_addr: address,
        user_data: None,
    };
    let transport = NetcodeClientTransport::new(
        Duration::from_secs_f64(js_sys::Date::now() / 1000.0),
        authentication,
        socket,
    )
    .map_err(|error| error.to_string())?;
    let client = RenetClient::new(
        ConnectionConfig::from_channels(channels.server_configs(), channels.client_configs()),
        false,
    );
    Ok((client, transport))
}

fn update_connection(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if world.resource::<Dashboard>().reconnect {
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
        let generation = {
            let mut dashboard = world.resource_mut::<Dashboard>();
            dashboard.reconnect = false;
            dashboard.humans.clear();
            dashboard.overview = WorldOverview::default();
            dashboard.selected = None;
            dashboard.fitted = false;
            dashboard.generation += 1;
            dashboard.started_at = now;
            dashboard.generation
        };
        let supported = web_sys::window().is_some_and(|window| window.is_secure_context())
            && webtransport_is_available_with_cert_hashes();
        if !supported {
            let mut dashboard = world.resource_mut::<Dashboard>();
            dashboard.phase = Phase::Unsupported;
            dashboard.message = "WebTransport with certificate hashes requires a supported browser and trusted HTTPS (localhost HTTP also works).".into();
            return;
        }
        let inbox = world.resource::<FetchInbox>().0.clone();
        {
            let mut dashboard = world.resource_mut::<Dashboard>();
            dashboard.phase = Phase::Loading;
            dashboard.message = "Loading connection configuration...".into();
        }
        spawn_local(async move {
            let result = fetch_configuration().await;
            inbox.lock().unwrap().push((generation, result));
        });
        return;
    }
    let results = std::mem::take(&mut *world.resource::<FetchInbox>().0.lock().unwrap());
    for (generation, result) in results {
        if generation == world.resource::<Dashboard>().generation
            && world.resource::<Dashboard>().phase == Phase::Loading
        {
            let connection = result
                .and_then(|config| make_connection(&config, world.resource::<RepliconChannels>()));
            match connection {
                Ok((client, transport)) => {
                    world.insert_resource(client);
                    world.insert_resource(transport);
                    let mut dashboard = world.resource_mut::<Dashboard>();
                    dashboard.phase = Phase::Connecting;
                    dashboard.message = "Connecting to world over WebTransport...".into();
                    dashboard.started_at = now;
                }
                Err(message) => {
                    let mut dashboard = world.resource_mut::<Dashboard>();
                    dashboard.phase = Phase::Failed;
                    dashboard.message = message;
                }
            }
        }
    }
    let connected = world
        .get_resource::<RenetClient>()
        .is_some_and(|client| client.is_connected());
    let disconnected = world
        .get_resource::<RenetClient>()
        .is_some_and(|client| client.is_disconnected());
    let phase = world.resource::<Dashboard>().phase;
    if connected && matches!(phase, Phase::Connecting | Phase::Connected) {
        let mut dashboard = world.resource_mut::<Dashboard>();
        dashboard.phase = Phase::Connected;
        dashboard.message = "Connected / Live world data / WebTransport".into();
    } else if disconnected && matches!(phase, Phase::Connected | Phase::Connecting) {
        let mut dashboard = world.resource_mut::<Dashboard>();
        dashboard.phase = Phase::Disconnected;
        dashboard.message = "Disconnected / Displayed data is stale. Restart world / rebuild Trunk if needed, then reconnect.".into();
    } else if matches!(phase, Phase::Loading | Phase::Connecting)
        && now - world.resource::<Dashboard>().started_at > 15.0
    {
        world.remove_resource::<NetcodeClientTransport>();
        world.remove_resource::<RenetClient>();
        world
            .resource_mut::<NextState<ClientState>>()
            .set(ClientState::Disconnected);
        let mut dashboard = world.resource_mut::<Dashboard>();
        dashboard.phase = Phase::Failed;
        dashboard.message = "Connection timed out. Check world, the LAN address, UDP port 5001, and the copied certificate hash; then reconnect.".into();
    }
}
