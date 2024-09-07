use hyperion::event::sync::SetUsernameEvent;

pub fn scramble_player_name(event: &mut SetUsernameEvent) {
    let mut characters: Vec<_> = event.username.chars().collect();
    fastrand::shuffle(&mut characters);

    event.username = characters.into_iter().collect();
}

