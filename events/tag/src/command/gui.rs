use clap::Parser;
use flecs_ecs::core::{Entity, World};
use hyperion_clap::{CommandPermission, MinecraftCommand};
use hyperion_gui::{ContainerType, Gui, GuiItem};
use hyperion_item::builder::ItemBuilder;

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "testgui")]
#[command_permission(group = "Normal")]
pub struct GuiCommand;

impl MinecraftCommand for GuiCommand {
    fn execute(self, world: &World, caller: Entity) {
        let mut gui = Gui::new(27, "Test Chest GUI".to_string(), ContainerType::Chest);

        let info_item = GuiItem::new(
            ItemBuilder::new(hyperion::ItemKind::GoldIngot)
                .name("Information")
                .glowing()
                .build(),
            |_player| {
                println!("Hello world!");
            },
        );

        gui.add_item(13, info_item).unwrap();
        gui.open(world, caller);
    }
}
