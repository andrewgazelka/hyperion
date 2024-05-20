use tracing::instrument;

use crate::{
    event::{ClickEvent, DropItem},
    net::Compose,
};

#[derive(Query)]
pub struct InventoryActionQuery<'a> {
    id: EntityId,
   // position: &'a mut CC,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut Packets,
    _player: With<&'static Player>,
}


#[instrument(skip_all, level = "trace")]
pub fn getasdf(r: Receiver<DropItem>, compose: Compose) {
    let x = 4;
    a

}
