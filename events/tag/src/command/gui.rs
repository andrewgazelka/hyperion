use clap::Parser;
use flecs_ecs::core::{Entity, EntityView};
use hyperion::valence_protocol::packets::play::click_slot_c2s::ClickMode;
use hyperion_clap::{CommandPermission, MinecraftCommand};
use hyperion_gui::{ContainerType, Gui, GuiItem};
use hyperion_item::builder::ItemBuilder;
use tracing::debug;

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "testgui")]
#[command_permission(group = "Normal")]
pub struct GuiCommand;

impl MinecraftCommand for GuiCommand {
    fn execute(self, system: EntityView<'_>, caller: Entity) {
        let mut gui = Gui::new(27, "Test Chest GUI".to_string(), ContainerType::Chest);

        let info_item = GuiItem::new(
            ItemBuilder::new(hyperion::ItemKind::GoldIngot)
                .name("Information")
                .glowing()
                .build(),
            |_player, click_mode| match click_mode {
                ClickMode::Click => debug!("Left Click"),
                ClickMode::ShiftClick => debug!("Shift Click"),
                ClickMode::Hotbar => debug!("Hotbar"),
                ClickMode::CreativeMiddleClick => debug!("Creative Middle Click"),
                ClickMode::DropKey => debug!("Drop Key"),
                ClickMode::Drag => debug!("Drag"),
                ClickMode::DoubleClick => debug!("Double Click"),
            },
        );

        gui.add_item(13, info_item).unwrap();

        gui.open(system, caller);
    }
}
