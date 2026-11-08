use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct GeneralName(pub usize);

#[derive(Component, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct GeneralKind(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Bounded<T> {
    value: T,
    min: T,
    max: T,
}

impl<T: PartialOrd + Copy> Bounded<T> {
    pub fn new(value: T, min: T, max: T) -> Self {
        let mut bounded = Self { value, min, max };
        bounded.set(value);
        bounded
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn min(&self) -> T {
        self.min
    }

    pub fn max(&self) -> T {
        self.max
    }

    pub fn set(&mut self, value: T) {
        self.value = if value < self.min {
            self.min
        } else if value > self.max {
            self.max
        } else {
            value
        };
    }
}
