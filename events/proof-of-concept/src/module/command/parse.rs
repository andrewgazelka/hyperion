use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::space1,
    combinator::{map, map_res, opt},
    error::ErrorKind,
    number::complete::float,
    sequence::preceded,
    IResult, Parser,
};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Stat {
    Armor,
    Toughness,
    Damage,
    Protection,
}

#[derive(Debug, PartialEq)]
pub enum ParsedCommand {
    Speed(f32),
    Team,
    Zombie,
    Dirt {
        x: i32,
        y: i32,
        z: i32,
    },
    Give {
        entity: String,
        item: String,
        count: i8,
    },
    Upgrade,
    Stats(Stat, f32),
    Health(f32),
    TpHere,
    Tp {
        x: f32,
        y: f32,
        z: f32,
    },
}

fn is_valid_player_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn space1_str(input: &str) -> IResult<&str, &str> {
    space1::<&str, nom::error::Error<&str>>(input)
}

fn parse_speed(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(preceded(tag("speed"), space1), float),
        ParsedCommand::Speed,
    )
    .parse(input)
}

fn parse_team(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("team"), |_| ParsedCommand::Team).parse(input)
}

fn parse_zombie(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("zombie"), |_| ParsedCommand::Zombie).parse(input)
}

fn parse_dirt(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(
            preceded(tag("dirt"), space1),
            (
                nom::character::complete::i32,
                preceded(space1, nom::character::complete::i32),
                preceded(space1, nom::character::complete::i32),
            ),
        ),
        |(x, y, z)| ParsedCommand::Dirt { x, y, z },
    )
    .parse(input)
}

fn parse_give(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        tuple((
            tag("give"),
            preceded(space1_str, take_while1(is_valid_player_char)),
            preceded(
                space1_str,
                preceded(opt(tag("minecraft:")), take_while1(is_valid_player_char)),
            ),
            preceded(space1_str, nom::character::complete::i8),
        )),
        |(_, entity, item, count)| ParsedCommand::Give {
            entity: entity.to_string(),
            item: item.to_string(),
            count,
        },
    )(input)
}

fn parse_upgrade(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("upgrade"), |_| ParsedCommand::Upgrade).parse(input)
}

fn parse_health(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(preceded(tag("health"), space1), float),
        ParsedCommand::Health,
    )
    .parse(input)
}

fn parse_stat(input: &str) -> IResult<&str, ParsedCommand> {
    map_res(
        preceded(
            preceded(tag("stat"), space1),
            (take_until(" "), preceded(space1, float)),
        ),
        |(stat, amount)| match stat {
            "armor" => Ok(ParsedCommand::Stats(Stat::Armor, amount)),
            "toughness" => Ok(ParsedCommand::Stats(Stat::Toughness, amount)),
            "damage" => Ok(ParsedCommand::Stats(Stat::Damage, amount)),
            "protection" => Ok(ParsedCommand::Stats(Stat::Protection, amount)),
            _ => Err("Invalid stat type"),
        },
    )
    .parse(input)
}

fn parse_tphere(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("tphere"), |_| ParsedCommand::TpHere).parse(input)
}

fn parse_tp(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(
            preceded(tag("tp"), space1),
            (float, preceded(space1, float), preceded(space1, float)),
        ),
        |(x, y, z)| ParsedCommand::Tp { x, y, z },
    )
    .parse(input)
}

pub fn command(input: &str) -> IResult<&str, ParsedCommand> {
    alt((
        parse_dirt,
        parse_give,
        parse_health,
        parse_speed,
        parse_stat,
        parse_team,
        parse_tphere,
        parse_tp,
        parse_upgrade,
        parse_zombie,
    ))
    .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_give_command() {
        let input = "give Cuz_Im_Clicks minecraft:dirt 64";
        let result = parse_give(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_give_command_no_minecraft() {
        let input = "give Cuz_Im_Clicks acacia_button 64";
        let result = parse_give(input);
        assert!(result.is_ok());
    }
}
