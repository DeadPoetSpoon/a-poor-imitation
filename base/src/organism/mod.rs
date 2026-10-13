use bevy::prelude::*;

use crate::general::{GeneralKind, GeneralName};

#[derive(Bundle)]
pub struct Person {
    pub name: GeneralName,
    pub kind: GeneralKind,
}
