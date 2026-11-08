// Behavioral Brain
pub mod behavioral_brain {
    use bevy::prelude::*;
    use serde::{Deserialize, Serialize};

    use crate::general::{Bounded, GeneralKind, GeneralName};
    #[derive(Component, Serialize, Deserialize, Reflect)]
    #[reflect(Component)]
    pub struct Breathing {
        respiratory_rate: Bounded<u8>,
        tidal_volume: Bounded<u8>,
    }

    impl Breathing {
        pub fn new(respiratory_rate: Bounded<u8>, tidal_volume: Bounded<u8>) -> Self {
            Self {
                respiratory_rate,
                tidal_volume,
            }
        }
    }

    #[derive(Serialize, Deserialize, Reflect)]
    pub enum Destination {
        Wander,
        Name(GeneralName),
        Kind(GeneralKind),
    }
    #[derive(Serialize, Deserialize, Reflect)]
    pub enum SpeedIntent {
        Accelerate,
        Keep,
        Decelerate,
    }
    #[derive(Component, Default, Serialize, Deserialize, Reflect)]
    #[reflect(Component)]
    pub struct Movement {
        destination: Option<Destination>,
        speed_intent: Option<SpeedIntent>,
    }
}
