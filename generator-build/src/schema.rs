#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MinecraftData {
    #[serde(rename = "minecraft:activity")]
    pub activity: ProtocolEntry,
    #[serde(rename = "minecraft:attribute")]
    pub attribute: ProtocolEntry,
    #[serde(rename = "minecraft:banner_pattern")]
    pub banner_pattern: ProtocolEntry,
    #[serde(rename = "minecraft:block")]
    pub block: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:block_entity_type")]
    pub block_entity_type: ProtocolEntry,
    #[serde(rename = "minecraft:block_predicate_type")]
    pub block_predicate_type: ProtocolEntry,
    #[serde(rename = "minecraft:cat_variant")]
    pub cat_variant: ProtocolEntry,
    #[serde(rename = "minecraft:chunk_status")]
    pub chunk_status: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:command_argument_type")]
    pub command_argument_type: ProtocolEntry,
    #[serde(rename = "minecraft:creative_mode_tab")]
    pub creative_mode_tab: ProtocolEntry,
    #[serde(rename = "minecraft:custom_stat")]
    pub custom_stat: ProtocolEntry,
    #[serde(rename = "minecraft:decorated_pot_patterns")]
    pub decorated_pot_patterns: ProtocolEntry,
    #[serde(rename = "minecraft:enchantment")]
    pub enchantment: ProtocolEntry,
    #[serde(rename = "minecraft:entity_type")]
    pub entity_type: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:float_provider_type")]
    pub float_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:fluid")]
    pub fluid: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:frog_variant")]
    pub frog_variant: ProtocolEntry,
    #[serde(rename = "minecraft:game_event")]
    pub game_event: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:height_provider_type")]
    pub height_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:instrument")]
    pub instrument: ProtocolEntry,
    #[serde(rename = "minecraft:int_provider_type")]
    pub int_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:item")]
    pub item: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:loot_condition_type")]
    pub loot_condition_type: ProtocolEntry,
    #[serde(rename = "minecraft:loot_function_type")]
    pub loot_function_type: ProtocolEntry,
    #[serde(rename = "minecraft:loot_nbt_provider_type")]
    pub loot_nbt_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:loot_number_provider_type")]
    pub loot_number_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:loot_pool_entry_type")]
    pub loot_pool_entry_type: ProtocolEntry,
    #[serde(rename = "minecraft:loot_score_provider_type")]
    pub loot_score_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:memory_module_type")]
    pub memory_module_type: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:menu")]
    pub menu: ProtocolEntry,
    #[serde(rename = "minecraft:mob_effect")]
    pub mob_effect: ProtocolEntry,
    #[serde(rename = "minecraft:painting_variant")]
    pub painting_variant: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:particle_type")]
    pub particle_type: ProtocolEntry,
    #[serde(rename = "minecraft:point_of_interest_type")]
    pub point_of_interest_type: ProtocolEntry,
    #[serde(rename = "minecraft:pos_rule_test")]
    pub pos_rule_test: ProtocolEntry,
    #[serde(rename = "minecraft:position_source_type")]
    pub position_source_type: ProtocolEntry,
    #[serde(rename = "minecraft:potion")]
    pub potion: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:recipe_serializer")]
    pub recipe_serializer: ProtocolEntry,
    #[serde(rename = "minecraft:recipe_type")]
    pub recipe_type: ProtocolEntry,
    #[serde(rename = "minecraft:rule_block_entity_modifier")]
    pub rule_block_entity_modifier: ProtocolEntry,
    #[serde(rename = "minecraft:rule_test")]
    pub rule_test: ProtocolEntry,
    #[serde(rename = "minecraft:schedule")]
    pub schedule: ProtocolEntry,
    #[serde(rename = "minecraft:sensor_type")]
    pub sensor_type: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:sound_event")]
    pub sound_event: ProtocolEntry,
    #[serde(rename = "minecraft:stat_type")]
    pub stat_type: ProtocolEntry,
    #[serde(rename = "minecraft:villager_profession")]
    pub villager_profession: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:villager_type")]
    pub villager_type: ProtocolEntryWithDefault,
    #[serde(rename = "minecraft:worldgen/biome_source")]
    pub worldgen_biome_source: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/block_state_provider_type")]
    pub worldgen_block_state_provider_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/carver")]
    pub worldgen_carver: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/chunk_generator")]
    pub worldgen_chunk_generator: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/density_function_type")]
    pub worldgen_density_function_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/feature")]
    pub worldgen_feature: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/feature_size_type")]
    pub worldgen_feature_size_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/foliage_placer_type")]
    pub worldgen_foliage_placer_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/material_condition")]
    pub worldgen_material_condition: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/material_rule")]
    pub worldgen_material_rule: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/placement_modifier_type")]
    pub worldgen_placement_modifier_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/root_placer_type")]
    pub worldgen_root_placer_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/structure_piece")]
    pub worldgen_structure_piece: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/structure_placement")]
    pub worldgen_structure_placement: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/structure_pool_element")]
    pub worldgen_structure_pool_element: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/structure_processor")]
    pub worldgen_structure_processor: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/structure_type")]
    pub worldgen_structure_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/tree_decorator_type")]
    pub worldgen_tree_decorator_type: ProtocolEntry,
    #[serde(rename = "minecraft:worldgen/trunk_placer_type")]
    pub worldgen_trunk_placer_type: ProtocolEntry,
}

#[derive(Debug, Deserialize)]
pub struct ProtocolEntry {
    pub entries: BTreeMap<String, ProtocolId>,
    #[serde(rename = "protocol_id")]
    pub protocol_id: i32,
}

#[derive(Debug, Deserialize)]
pub struct ProtocolEntryWithDefault {
    pub default: String,
    pub entries: BTreeMap<String, ProtocolId>,
    #[serde(rename = "protocol_id")]
    pub protocol_id: i32,
}

#[derive(Debug, Deserialize, Copy, Clone)]
pub struct ProtocolId {
    #[serde(rename = "protocol_id")]
    pub protocol_id: i32,
}
