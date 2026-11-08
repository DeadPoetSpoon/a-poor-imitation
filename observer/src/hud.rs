use std::time::Duration;

use base::{HumanId, HumanStatus, PORT, PersonPosition, WorldOverview, general::GeneralName};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
    time::common_conditions::on_real_timer,
};
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::{netcode::NetcodeClientTransport, renet2::RenetClient};

pub(super) struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_hud)
            .add_systems(
                PostUpdate,
                update_hud.run_if(on_real_timer(Duration::from_millis(250))),
            );
    }
}

#[derive(Component)]
struct HudText;

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                left: px(12),
                max_width: percent(100),
                padding: UiRect::all(px(12)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.04, 0.06, 0.85)),
        ))
        .with_children(|parent| {
            parent.spawn((
                HudText,
                Text::new("OBSERVER\nStarting..."),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.87, 0.92, 0.97)),
            ));
        });
}

fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    state: Res<State<ClientState>>,
    client: Option<Res<RenetClient>>,
    transport: Option<Res<NetcodeClientTransport>>,
    people: Query<(&GeneralName, &PersonPosition, Option<&HumanStatus>), With<HumanId>>,
    overview: Query<&WorldOverview>,
    mut text: Single<&mut Text, With<HudText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed())
        .map_or_else(|| "--".into(), |value| format!("{value:.0}"));
    let frame_time = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.smoothed())
        .map_or_else(|| "--".into(), |value| format!("{value:.2}"));
    let connected = *state.get() == ClientState::Connected;
    let network = client
        .as_ref()
        .filter(|_| connected)
        .map(|client| client.network_info())
        .map_or_else(
            || "RTT: -- ms | Loss: --%\nTX: -- KiB/s | RX: -- KiB/s".into(),
            |info| {
                format!(
                    "RTT: {:.1} ms | Loss: {:.1}%\nTX: {:.2} KiB/s | RX: {:.2} KiB/s",
                    info.rtt * 1000.0,
                    info.packet_loss * 100.0,
                    info.bytes_sent_per_second / 1024.0,
                    info.bytes_received_per_second / 1024.0,
                )
            },
        );
    let client_id = transport.as_ref().map(|transport| transport.client_id());
    let identity = client_id.map_or_else(|| "--".into(), |id| id.to_string());
    let character = if connected {
        people
            .iter()
            .find(|(_, _, status)| {
                status.is_some_and(|status| status.owner.is_some() && status.owner == client_id)
            })
            .map_or_else(
                || "You: Spectator / awaiting assignment".into(),
                |(name, position, _)| {
                    format!(
                        "You: {} | X: {:.1} Y: {:.1}",
                        name.0, position.x, position.y,
                    )
                },
            )
    } else {
        "You: --".into()
    };
    let world = overview.single().ok().filter(|_| connected).map_or_else(
        || "Online: -- | Players: -- | Dashboards: -- | Consoles: --".into(),
        |overview| {
            format!(
                "Online: {} | Players: {} | Dashboards: {} | Consoles: {}",
                overview.total_connections,
                overview.player_connections,
                overview.dashboard_connections,
                overview.console_connections,
            )
        },
    );

    text.0 = format!(
        "FPS: {fps} | Frame: {frame_time} ms\n\
         {:?} | 127.0.0.1:{PORT}\n\
         {network}\n\
         Client: {identity}\n\
         {world}\n\
         Humans: {} | {character}\n\
         WASD: Move",
        state.get(),
        if connected { people.iter().count() } else { 0 },
    );
}
