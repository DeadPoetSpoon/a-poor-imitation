use std::collections::HashMap;

use base::{HumanId, HumanStatus, Person, PersonPosition, WorldOverview};
use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
};
use bevy_replicon::prelude::*;

const PANEL: Color = Color::srgb(0.055, 0.08, 0.12);
const TEXT: Color = Color::srgb(0.87, 0.92, 0.97);
const MUTED: Color = Color::srgb(0.52, 0.63, 0.74);
const ACCENT: Color = Color::srgb(0.30, 0.90, 0.72);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Loading,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct HumanView {
    id: u64,
    name: String,
    position: Vec2,
    status: HumanStatus,
}

#[derive(Resource)]
pub(crate) struct Dashboard {
    pub(crate) phase: Phase,
    pub(crate) message: String,
    pub(crate) humans: Vec<HumanView>,
    pub(crate) overview: WorldOverview,
    pub(crate) selected: Option<u64>,
    pub(crate) generation: u64,
    pub(crate) started_at: f64,
    pub(crate) fitted: bool,
    pub(crate) reconnect: bool,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            phase: Phase::Loading,
            message: "Loading connection configuration...".into(),
            humans: Vec::new(),
            overview: WorldOverview::default(),
            selected: None,
            generation: 0,
            started_at: 0.0,
            fitted: false,
            reconnect: true,
        }
    }
}

#[derive(Resource)]
struct MapView {
    center: Vec2,
    zoom: f32,
    rect: Rect,
    window_size: Vec2,
    mouse_drag: Option<(Vec2, Vec2)>,
    previous_touches: Vec<(u64, Vec2)>,
    touch_dragged: bool,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            rect: Rect::default(),
            window_size: Vec2::ZERO,
            mouse_drag: None,
            previous_touches: Vec::new(),
            touch_dragged: false,
        }
    }
}

impl MapView {
    fn screen(&self, position: Vec2) -> Vec2 {
        let offset = (position - self.center) * self.zoom;
        self.rect.center() + Vec2::new(offset.x, -offset.y)
    }

    fn world(&self, screen: Vec2) -> Vec2 {
        let offset = (screen - self.rect.center()) / self.zoom;
        self.center + Vec2::new(offset.x, -offset.y)
    }

    fn canvas(&self, screen: Vec2) -> Vec2 {
        Vec2::new(
            screen.x - self.window_size.x / 2.0,
            self.window_size.y / 2.0 - screen.y,
        )
    }

    fn pan(&mut self, delta: Vec2) {
        self.center -= Vec2::new(delta.x, -delta.y) / self.zoom;
    }

    fn zoom_at(&mut self, factor: f32, screen: Vec2) {
        let anchor = self.world(screen);
        self.zoom = (self.zoom * factor).clamp(0.02, 12.0);
        self.center += anchor - self.world(screen);
    }

    fn fit(&mut self, humans: &[HumanView]) {
        let positions: Vec<_> = humans
            .iter()
            .map(|human| human.position)
            .filter(|position| position.is_finite())
            .collect();
        if positions.is_empty() || self.rect.width() < 1.0 || self.rect.height() < 1.0 {
            return;
        }
        let minimum = positions.iter().copied().reduce(Vec2::min).unwrap();
        let maximum = positions.iter().copied().reduce(Vec2::max).unwrap();
        self.center = (minimum + maximum) / 2.0;
        let extent = (maximum - minimum + Vec2::splat(160.0)).max(Vec2::splat(240.0));
        self.zoom = (self.rect.width() / extent.x)
            .min(self.rect.height() / extent.y)
            .clamp(0.02, 3.0)
            * 0.8;
    }
}

#[derive(Component, Clone, Copy)]
enum Action {
    Fit,
    ZoomIn,
    ZoomOut,
    Reconnect,
    Select(u64),
}

#[derive(Component)]
enum DashboardText {
    Status,
    Counts,
    Selection,
    Row(u64),
}

#[derive(Component)]
struct MapLabel(u64);

#[derive(Resource)]
struct UiHandles {
    body: Entity,
    map: Entity,
    sidebar: Entity,
    list: Entity,
    rows: HashMap<u64, Entity>,
    labels: HashMap<u64, Entity>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DashboardSystems {
    Actions,
    Connection,
    Presentation,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Dashboard>()
        .init_resource::<MapView>()
        .configure_sets(
            Update,
            (
                DashboardSystems::Actions,
                DashboardSystems::Connection,
                DashboardSystems::Presentation,
            )
                .chain(),
        )
        .add_systems(Startup, setup_ui)
        .add_systems(Update, handle_buttons.in_set(DashboardSystems::Actions))
        .add_systems(
            Update,
            (
                capture_world,
                update_layout,
                map_input,
                scroll_list,
                sync_entities,
                draw_map,
                update_text,
                update_labels,
            )
                .chain()
                .in_set(DashboardSystems::Presentation),
        );
}

fn button(commands: &mut Commands, parent: Entity, text: &str, action: Action) -> Entity {
    let entity = commands
        .spawn((
            Button,
            action,
            Node {
                padding: UiRect::axes(px(14), px(9)),
                min_height: px(42),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.18, 0.25)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(text),
                TextFont::from_font_size(15.0),
                TextColor(TEXT),
            ));
        })
        .id();
    commands.entity(parent).add_child(entity);
    entity
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);
    let root = commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .id();
    let header = commands
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::all(px(16)),
                row_gap: px(6),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("TOUCHPAD / WORLD"),
                TextFont::from_font_size(22.0),
                TextColor(ACCENT),
            ));
            parent.spawn((
                Text::new("Loading..."),
                TextFont::from_font_size(14.0),
                TextColor(MUTED),
                DashboardText::Status,
            ));
            parent.spawn((
                Text::new("Waiting for world information"),
                TextFont::from_font_size(15.0),
                TextColor(TEXT),
                DashboardText::Counts,
            ));
        })
        .id();
    let body = commands
        .spawn(Node {
            flex_grow: 1.0,
            min_height: px(0),
            width: percent(100),
            ..default()
        })
        .id();
    let map = commands
        .spawn(Node {
            flex_grow: 1.0,
            min_width: px(0),
            min_height: px(0),
            ..default()
        })
        .id();
    let sidebar = commands
        .spawn((
            Node {
                width: px(330),
                flex_shrink: 0.0,
                min_height: px(0),
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("HUMANS"),
                TextFont::from_font_size(18.0),
                TextColor(ACCENT),
            ));
            parent.spawn((
                Text::new("Select a human on the map or in this list."),
                TextFont::from_font_size(14.0),
                TextColor(MUTED),
                DashboardText::Selection,
            ));
        })
        .id();
    let controls = commands
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(8),
            row_gap: px(8),
            flex_shrink: 0.0,
            ..default()
        })
        .id();
    button(&mut commands, controls, "Fit all", Action::Fit);
    button(&mut commands, controls, "+", Action::ZoomIn);
    button(&mut commands, controls, "-", Action::ZoomOut);
    button(&mut commands, controls, "Reconnect", Action::Reconnect);
    let list = commands
        .spawn((
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .id();
    let footer = commands
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::axes(px(16), px(8)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Drag to pan / Scroll or pinch to zoom / Tap to inspect / Read-only"),
                TextFont::from_font_size(12.0),
                TextColor(MUTED),
            ));
        })
        .id();
    commands.entity(root).add_children(&[header, body, footer]);
    commands.entity(body).add_children(&[map, sidebar]);
    commands.entity(sidebar).add_children(&[controls, list]);
    commands.insert_resource(UiHandles {
        body,
        map,
        sidebar,
        list,
        rows: HashMap::new(),
        labels: HashMap::new(),
    });
}

fn handle_buttons(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut colors: Query<(&Interaction, &Action, &mut BackgroundColor)>,
    mut dashboard: ResMut<Dashboard>,
    mut map: ResMut<MapView>,
) {
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Fit => map.fit(&dashboard.humans),
            Action::ZoomIn => {
                let center = map.rect.center();
                map.zoom_at(1.25, center);
            }
            Action::ZoomOut => {
                let center = map.rect.center();
                map.zoom_at(0.8, center);
            }
            Action::Reconnect => {
                if !matches!(dashboard.phase, Phase::Loading | Phase::Connecting) {
                    dashboard.reconnect = true;
                }
            }
            Action::Select(id) => dashboard.selected = Some(*id),
        }
    }
    for (interaction, action, mut color) in &mut colors {
        let selected = matches!(action, Action::Select(id) if dashboard.selected == Some(*id));
        color.0 = if selected || *interaction == Interaction::Pressed {
            Color::srgb(0.12, 0.35, 0.30)
        } else if *interaction == Interaction::Hovered {
            Color::srgb(0.19, 0.27, 0.36)
        } else {
            Color::srgb(0.12, 0.18, 0.25)
        };
    }
}

fn capture_world(
    state: Res<State<ClientState>>,
    people: Query<(&HumanId, &Person, &PersonPosition, &HumanStatus)>,
    overview: Query<&WorldOverview>,
    mut dashboard: ResMut<Dashboard>,
) {
    if dashboard.phase != Phase::Connected || *state.get() != ClientState::Connected {
        return;
    }
    dashboard.humans = people
        .iter()
        .map(|(id, person, position, status)| HumanView {
            id: id.0,
            name: person.name.clone(),
            position: position.0,
            status: *status,
        })
        .collect();
    dashboard.humans.sort_by_key(|human| human.id);
    if let Some(overview) = overview.iter().next() {
        dashboard.overview = *overview;
    }
    if dashboard
        .selected
        .is_some_and(|id| !dashboard.humans.iter().any(|human| human.id == id))
    {
        dashboard.selected = None;
    }
}

fn update_layout(
    windows: Query<&Window>,
    handles: Res<UiHandles>,
    mut nodes: Query<&mut Node>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut map: ResMut<MapView>,
    mut dashboard: ResMut<Dashboard>,
) {
    let Some(window) = windows.iter().next() else {
        return;
    };
    map.window_size = Vec2::new(window.width(), window.height());
    let narrow = window.width() < 760.0;
    if let Ok(mut body) = nodes.get_mut(handles.body) {
        body.flex_direction = if narrow {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
    }
    if let Ok(mut sidebar) = nodes.get_mut(handles.sidebar) {
        sidebar.width = if narrow { percent(100) } else { px(330) };
        sidebar.height = if narrow { percent(43) } else { percent(100) };
    }
    if let Ok((node, transform)) = geometry.get(handles.map) {
        let scale = node.inverse_scale_factor();
        let center = transform.affine().translation * scale;
        map.rect = Rect::from_center_size(center, node.size() * scale);
    }
    if !dashboard.fitted
        && !dashboard.humans.is_empty()
        && map.rect.width() > 1.0
        && map.rect.height() > 1.0
    {
        map.fit(&dashboard.humans);
        dashboard.fitted = true;
    }
}

fn select_at(screen: Vec2, map: &MapView, dashboard: &mut Dashboard) {
    dashboard.selected = dashboard
        .humans
        .iter()
        .filter(|human| human.position.is_finite())
        .min_by(|left, right| {
            map.screen(left.position)
                .distance_squared(screen)
                .total_cmp(&map.screen(right.position).distance_squared(screen))
        })
        .filter(|human| map.screen(human.position).distance(screen) <= 24.0)
        .map(|human| human.id);
}

fn map_input(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut wheel: MessageReader<MouseWheel>,
    mut map: ResMut<MapView>,
    mut dashboard: ResMut<Dashboard>,
) {
    let Some(window) = windows.iter().next() else {
        return;
    };
    let cursor = window.cursor_position();
    for event in wheel.read() {
        if let Some(cursor) = cursor.filter(|cursor| map.rect.contains(*cursor)) {
            let delta = if event.unit == MouseScrollUnit::Line {
                event.y * 30.0
            } else {
                event.y
            };
            map.zoom_at((delta * 0.004).exp(), cursor);
        }
    }
    if touches.iter().next().is_none() {
        if mouse.just_pressed(MouseButton::Left) {
            map.mouse_drag = cursor
                .filter(|cursor| map.rect.contains(*cursor))
                .map(|cursor| (cursor, cursor));
        }
        if mouse.pressed(MouseButton::Left)
            && let (Some((origin, previous)), Some(cursor)) = (map.mouse_drag, cursor)
        {
            map.pan(cursor - previous);
            map.mouse_drag = Some((origin, cursor));
        }
        if mouse.just_released(MouseButton::Left)
            && let (Some((origin, previous)), Some(cursor)) = (map.mouse_drag.take(), cursor)
            && origin.distance(previous) < 6.0
            && map.rect.contains(cursor)
        {
            select_at(cursor, &map, &mut dashboard);
        }
    } else {
        map.mouse_drag = None;
    }
    let mut active: Vec<_> = touches
        .iter()
        .filter(|touch| map.rect.contains(touch.start_position()))
        .map(|touch| (touch.id(), touch.position()))
        .collect();
    active.sort_by_key(|(id, _)| *id);
    if active.len() >= 2 {
        map.touch_dragged = true;
        if map.previous_touches.len() >= 2
            && active[0].0 == map.previous_touches[0].0
            && active[1].0 == map.previous_touches[1].0
        {
            let previous_center = (map.previous_touches[0].1 + map.previous_touches[1].1) / 2.0;
            let center = (active[0].1 + active[1].1) / 2.0;
            let previous_distance = map.previous_touches[0]
                .1
                .distance(map.previous_touches[1].1);
            let distance = active[0].1.distance(active[1].1);
            if previous_distance > 1.0 {
                map.zoom_at(distance / previous_distance, previous_center);
            }
            map.pan(center - previous_center);
        }
    } else if let Some((id, position)) = active.first() {
        if let Some((_, previous)) = map
            .previous_touches
            .iter()
            .find(|(previous_id, _)| previous_id == id)
        {
            let delta = *position - *previous;
            map.pan(delta);
        }
        if let Some(touch) = touches.get_pressed(*id)
            && touch.start_position().distance(*position) >= 6.0
        {
            map.touch_dragged = true;
        }
    }
    for touch in touches.iter_just_released() {
        if !map.touch_dragged
            && map.rect.contains(touch.start_position())
            && touch.start_position().distance(touch.position()) < 6.0
        {
            select_at(touch.position(), &map, &mut dashboard);
        }
    }
    if active.is_empty() {
        map.touch_dragged = false;
    }
    map.previous_touches = active;
}

fn scroll_list(
    windows: Query<&Window>,
    touches: Res<Touches>,
    mut wheel: MessageReader<MouseWheel>,
    handles: Res<UiHandles>,
    mut lists: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition)>,
    mut previous_touches: Local<HashMap<u64, Vec2>>,
) {
    let Some(window) = windows.iter().next() else {
        return;
    };
    let Ok((node, transform, mut scroll)) = lists.get_mut(handles.list) else {
        return;
    };
    let scale = node.inverse_scale_factor();
    let rect = Rect::from_center_size(transform.affine().translation * scale, node.size() * scale);
    let mut delta = 0.0;
    for event in wheel.read() {
        if window
            .cursor_position()
            .is_some_and(|cursor| rect.contains(cursor))
        {
            delta -= event.y
                * if event.unit == MouseScrollUnit::Line {
                    28.0
                } else {
                    1.0
                };
        }
    }
    for touch in touches.iter() {
        if rect.contains(touch.start_position())
            && let Some(previous) = previous_touches.get(&touch.id())
        {
            delta -= touch.position().y - previous.y;
        }
        previous_touches.insert(touch.id(), touch.position());
    }
    previous_touches.retain(|id, _| touches.get_pressed(*id).is_some());
    let maximum = ((node.content_size().y - node.size().y) * scale).max(0.0);
    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
}

fn human_color(id: u64) -> Color {
    Color::hsl((id.wrapping_mul(137) % 360) as f32, 0.68, 0.65)
}

fn sync_entities(
    mut commands: Commands,
    mut handles: ResMut<UiHandles>,
    dashboard: Res<Dashboard>,
) {
    handles.rows.retain(|id, entity| {
        let keep = dashboard.humans.iter().any(|human| human.id == *id);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    handles.labels.retain(|id, entity| {
        let keep = dashboard.humans.iter().any(|human| human.id == *id);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    for human in &dashboard.humans {
        if !handles.rows.contains_key(&human.id) {
            let entity = commands
                .spawn((
                    Button,
                    Action::Select(human.id),
                    Node {
                        padding: UiRect::all(px(12)),
                        width: percent(100),
                        min_height: px(94),
                        flex_shrink: 0.0,
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.18, 0.25)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new(&human.name),
                        TextFont::from_font_size(15.0),
                        TextColor(human_color(human.id)),
                        DashboardText::Row(human.id),
                    ));
                })
                .id();
            commands.entity(handles.list).add_child(entity);
            handles.rows.insert(human.id, entity);
        }
        handles.labels.entry(human.id).or_insert_with(|| {
            commands
                .spawn((
                    Text2d::new(&human.name),
                    TextFont::from_font_size(14.0),
                    TextColor(human_color(human.id)),
                    MapLabel(human.id),
                ))
                .id()
        });
    }
}

fn draw_map(mut gizmos: Gizmos, map: Res<MapView>, dashboard: Res<Dashboard>) {
    if map.rect.width() <= 1.0 || map.rect.height() <= 1.0 {
        return;
    }
    let minimum = map.world(map.rect.min);
    let maximum = map.world(map.rect.max);
    let step = 10.0_f32.powf((70.0 / map.zoom).log10().ceil());
    let grid_color = Color::srgb(0.09, 0.14, 0.20);
    let mut coordinate = (minimum.x / step).floor() * step;
    for _ in 0..100 {
        if coordinate > maximum.x {
            break;
        }
        let screen = map.screen(Vec2::new(coordinate, 0.0)).x;
        if screen >= map.rect.min.x {
            gizmos.line_2d(
                map.canvas(Vec2::new(screen, map.rect.min.y)),
                map.canvas(Vec2::new(screen, map.rect.max.y)),
                grid_color,
            );
        }
        coordinate += step;
    }
    coordinate = (maximum.y / step).floor() * step;
    for _ in 0..100 {
        if coordinate > minimum.y {
            break;
        }
        let screen = map.screen(Vec2::new(0.0, coordinate)).y;
        if screen <= map.rect.max.y {
            gizmos.line_2d(
                map.canvas(Vec2::new(map.rect.min.x, screen)),
                map.canvas(Vec2::new(map.rect.max.x, screen)),
                grid_color,
            );
        }
        coordinate += step;
    }
    let origin = map.screen(Vec2::ZERO);
    if map.rect.contains(origin) {
        gizmos.cross_2d(map.canvas(origin), 8.0, MUTED);
    }
    for human in &dashboard.humans {
        if !human.position.is_finite() {
            continue;
        }
        let screen = map.screen(human.position);
        if !map.rect.inflate(-25.0).contains(screen) {
            continue;
        }
        let color = human_color(human.id);
        let radius = if human.status.moving { 15.0 } else { 11.0 };
        gizmos.circle_2d(map.canvas(screen), radius, color);
        if human.status.moving {
            gizmos.circle_2d(map.canvas(screen), 19.0, color);
        }
        if dashboard.selected == Some(human.id) {
            gizmos.rect_2d(map.canvas(screen), Vec2::splat(46.0), ACCENT);
        }
    }
}

fn update_text(
    dashboard: Res<Dashboard>,
    mut texts: Query<(&DashboardText, &mut Text, &mut TextColor)>,
) {
    for (kind, mut text, mut color) in &mut texts {
        let value = match kind {
            DashboardText::Status => {
                color.0 = if dashboard.phase == Phase::Connected {
                    ACCENT
                } else {
                    MUTED
                };
                dashboard.message.clone()
            }
            DashboardText::Counts => format!(
                "{} humans   /   {} clients   /   {} players   /   {} dashboards{}",
                dashboard.humans.len(),
                dashboard.overview.total_connections,
                dashboard.overview.player_connections,
                dashboard.overview.dashboard_connections,
                if matches!(dashboard.phase, Phase::Disconnected | Phase::Failed)
                    && !dashboard.humans.is_empty()
                {
                    "   /   STALE"
                } else {
                    ""
                }
            ),
            DashboardText::Selection => dashboard
                .selected
                .and_then(|id| dashboard.humans.iter().find(|human| human.id == id))
                .map(|human| format!("Selected: {} / ID {}", human.name, human.id))
                .unwrap_or_else(|| "Select a human on the map or in this list.".into()),
            DashboardText::Row(id) => {
                let Some(human) = dashboard.humans.iter().find(|human| human.id == *id) else {
                    continue;
                };
                format!(
                    "{}  /  {}\nPosition  {:.1}, {:.1}\nOwner  {}",
                    human.name,
                    if human.status.moving {
                        "MOVING"
                    } else {
                        "IDLE"
                    },
                    human.position.x,
                    human.position.y,
                    human
                        .status
                        .owner
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "Unassigned".into())
                )
            }
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn update_labels(
    map: Res<MapView>,
    dashboard: Res<Dashboard>,
    mut labels: Query<(&MapLabel, &mut Text2d, &mut Transform, &mut Visibility)>,
) {
    for (label, mut text, mut transform, mut visibility) in &mut labels {
        let Some(human) = dashboard.humans.iter().find(|human| human.id == label.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let screen = map.screen(human.position) + Vec2::new(0.0, 33.0);
        let visible = human.position.is_finite() && map.rect.inflate(-45.0).contains(screen);
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        transform.translation = map.canvas(screen).extend(1.0);
        if text.0 != human.name {
            text.0.clone_from(&human.name);
        }
    }
}
