use server::evenio::component::Component;

#[derive(Component, PartialOrd, PartialEq, Debug, Eq, Ord, Hash, Copy, Clone)]
pub enum Team {
    Human,
    Zombie,
}

const _: () = assert!(std::mem::size_of::<Team>() == 1);
