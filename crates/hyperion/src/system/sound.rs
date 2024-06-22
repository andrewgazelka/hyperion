use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::system,
};
use valence_protocol::{
    ident,
    packets::play,
    sound::{SoundCategory, SoundId},
};

use crate::{
    component::Pose,
    net::{Compose, NetworkStreamRef},
};

pub fn sound(world: &World) {
    system!(
        "sound",
        world,
        &Compose($),
        &Pose,
        &NetworkStreamRef,
    )
    .each(|(compose, pose, net)| {
        // https://www.digminecraft.com/lists/sound_list_pc.php
        let id = SoundId::Direct {
            id: ident!("block.note_block.bit").into(),
            range: None,
        };

        // position * 8
        let position = (pose.position * 8.0).as_ivec3();

        // 0.5 to 2.0
        let pitch = fastrand::f32().mul_add(0.5, 0.5);

        let pkt = play::PlaySoundS2c {
            id,
            category: SoundCategory::Master,
            position,
            volume: 1.0,
            pitch,
            seed: 0,
        };

        compose.unicast(&pkt, net).unwrap();
    });
}
