use evenio::event::Receiver;
use tracing::instrument;
use valence_protocol::packets::{
    play,
    play::entity_attributes_s2c::{AttributeModifier, AttributeProperty},
};
use valence_server::Ident;

use crate::{
    components::Uuid,
    event,
    event::SpeedEffect,
    net::{Compose, StreamId},
};

#[instrument(skip_all, level = "trace")]
pub fn display(r: Receiver<event::DisplayPotionEffect, &mut StreamId>, compose: Compose) {
    let &event::DisplayPotionEffect {
        effect,
        amplifier,
        duration,
        ambient,
        show_particles,
        show_icon,
        ..
    } = r.event;

    let packets = r.query;

    // let entity_id = target.index().0 as i32;
    let entity_id = 0;

    let effect_id = i32::from(effect.to_raw());

    let pkt = play::EntityStatusEffectS2c {
        entity_id: entity_id.into(),
        effect_id: effect_id.into(),
        amplifier,
        duration: duration.into(),
        flags: play::entity_status_effect_s2c::Flags::new()
            .with_show_icon(show_icon)
            .with_is_ambient(ambient)
            .with_show_particles(show_particles),

        // todo: what in the world is a factor codec?
        factor_codec: None,
    };

    compose.unicast(&pkt, packets).unwrap();
}

pub fn speed(r: Receiver<SpeedEffect, (&mut StreamId, &Uuid)>, compose: Compose) {
    // speed 1 - > 0.10000000149011612

    // eneric.max_health	20.0	1.0	1024.0	Max Health.
    // generic.follow_range	32.0	0.0	2048.0	Follow Range.
    // generic.knockback_resistance	0.0	0.0	1.0	Knockback Resistance.
    // generic.movement_speed	0.7	0.0	1024.0	Movement Speed.
    // generic.flying_speed	0.4	0.0	1024.0	Flying Speed.
    // generic.attack_damage	2.0	0.0	2048.0	Attack Damage.
    // generic.attack_knockback	0.0	0.0	5.0	—
    // generic.attack_speed	4.0	0.0	1024.0	Attack Speed.
    // generic.armor	0.0	0.0	30.0	Armor.
    // generic.armor_toughness	0.0	0.0	20.0	Armor Toughness.
    // generic.luck	0.0	-1024.0	1024.0	Luck.
    // zombie.spawn_reinforcements	0.0	0.0	1.0	Spawn Reinforcements Chance.
    // horse.jump_strength	0.7	0.0	2.0	Jump Strength.
    // generic.reachDistance	5.0	0.0	1024.0	Player Reach Distance (Forge only).
    // forge.swimSpeed

    let effect = r.event;

    let amplifier = effect.level() + 1;
    let (packets, uuid) = r.query;

    let modifier = AttributeModifier {
        uuid: uuid.0,
        amount: 0.2 * f64::from(amplifier),

        // todo: what is operation? it is always 2 for speed... set?
        operation: 2,
    };

    let prop = AttributeProperty {
        key: Ident::new("minecraft:generic.movement_speed").unwrap(),
        // 0.2 probably is the slope from mc source code?
        //     public static final Potion moveSpeed = (new Potion(1, new ResourceLocation("speed"), false, 8171462)).setPotionName("potion.moveSpeed").setIconIndex(0, 0).registerPotionAttributeModifier(SharedMonsterAttributes.movementSpeed, "91AEAA56-376B-4498-935B-2F7F68070635", 0.20000000298023224D, 2);
        // value: 0.7 * f64::from(amplifier) * 0.2,
        value: 0.100_000_001_490_116_12,
        modifiers: vec![modifier],
    };

    let entity_id = 0;

    let pkt = play::EntityAttributesS2c {
        entity_id: entity_id.into(),

        // todo: remove vec 😭 and use a Cow 🐮
        properties: vec![prop],
    };

    compose.unicast(&pkt, packets).unwrap();
}
