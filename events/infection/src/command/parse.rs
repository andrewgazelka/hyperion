use nom::{
    branch::alt, bytes::complete::tag, character::complete::space1, combinator::map,
    number::complete::float, sequence::{preceded, tuple}, IResult,
};

#[derive(Debug, PartialEq)]
pub enum ParsedCommand {
    Speed(f32),
    Team,
    Zombie,
    Dirt { x: i32, y: i32, z: i32 },
}

fn parse_speed(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(preceded(tag("speed"), space1), float),
        ParsedCommand::Speed,
    )(input)
}

fn parse_team(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("team"), |_| ParsedCommand::Team)(input)
}

fn parse_zombie(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("zombie"), |_| ParsedCommand::Zombie)(input)
}

fn parse_dirt(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(
            preceded(tag("dirt"), space1),
            tuple((
                nom::character::complete::i32,
                preceded(space1, nom::character::complete::i32),
                preceded(space1, nom::character::complete::i32),
            )),
        ),
        |(x, y, z)| ParsedCommand::Dirt { x, y, z },
    )(input)
}

pub fn command(input: &str) -> IResult<&str, ParsedCommand> {
    alt((parse_speed, parse_team, parse_zombie, parse_dirt))(input)
}
