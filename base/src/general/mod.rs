use bevy::prelude::*;

#[derive(Component)]
pub struct GeneralName(pub usize);

#[derive(Component)]
pub enum GeneralKind {
    Human(usize),
}
