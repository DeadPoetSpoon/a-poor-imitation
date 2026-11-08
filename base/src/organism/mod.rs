pub mod brain;
use crate::{
    general::{GeneralKind, GeneralName},
    organism,
};
use bevy::prelude::*;
pub use brain::*;

#[derive(Bundle)]
pub struct Human {
    pub name: GeneralName,
    pub kind: GeneralKind,
    pub breathing: organism::behavioral_brain::Breathing,
    pub movement: organism::behavioral_brain::Movement,
}
