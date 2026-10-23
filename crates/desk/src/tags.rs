//! Reading tags in what a person typed: `@person` for their machine, `@person/place` for one of
//! their places. Only people who have joined, and places they advertise, count.

use player_two_core::api::Person;
use player_two_core::event::Tag;
use player_two_core::names::{PersonName, PlaceName};

fn is_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

fn is_place_char(c: char) -> bool {
    is_name_char(c) || c == '.' || c == '_'
}

/// Every `@person` or `@person/place` in `text` that names someone who has joined and a place of
/// theirs, in order, once each. An `@` must start the text or follow a space.
#[must_use]
pub fn read(text: &str, people: &[Person]) -> Vec<Tag> {
    let mut tags: Vec<Tag> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let starts = chars[at] == '@' && (at == 0 || chars[at - 1].is_whitespace());
        if !starts {
            at += 1;
            continue;
        }
        let name_end = (at + 1..chars.len())
            .find(|&i| !is_name_char(chars[i]))
            .unwrap_or(chars.len());
        let name: String = chars[at + 1..name_end].iter().collect();
        let (place, end) = if chars.get(name_end) == Some(&'/') {
            let place_end = (name_end + 1..chars.len())
                .find(|&i| !is_place_char(chars[i]))
                .unwrap_or(chars.len());
            // A place does not end with a dot: "@bob/uploader." ends a sentence.
            let mut place: String = chars[name_end + 1..place_end].iter().collect();
            while place.ends_with('.') {
                place.pop();
            }
            (Some(place), place_end)
        } else {
            (None, name_end)
        };
        at = end.max(at + 1);
        let Ok(person) = PersonName::new(name) else {
            continue;
        };
        let Some(known) = people.iter().find(|known| known.name == person) else {
            continue;
        };
        let place = match place {
            None => PlaceName::machine(),
            Some(place) => match PlaceName::new(place) {
                Ok(place) if known.places.iter().any(|offered| offered.name == place) => place,
                _ => continue,
            },
        };
        let tag = Tag { person, place };
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use player_two_core::api::{Person, PlaceInfo, PlaceKind};
    use player_two_core::names::{PersonName, PlaceName};

    use super::read;

    fn person(name: &str, places: &[&str]) -> Person {
        Person {
            name: PersonName::new(name).expect("a name"),
            here: true,
            last_seen: None,
            places: places
                .iter()
                .map(|place| PlaceInfo {
                    name: PlaceName::new(*place).expect("a place"),
                    kind: if *place == "machine" {
                        PlaceKind::Machine
                    } else {
                        PlaceKind::Repo
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn tags_name_people_and_their_places() {
        let people = [
            person("alice", &["machine", "uploader"]),
            person("bob", &["machine"]),
        ];
        let read = |text: &str| -> Vec<String> {
            read(text, &people)
                .iter()
                .map(|tag| format!("{}/{}", tag.person, tag.place))
                .collect()
        };
        assert_eq!(read("@alice fix pip please"), ["alice/machine"]);
        assert_eq!(read("look at @alice/uploader."), ["alice/uploader"]);
        assert_eq!(
            read("@bob and @alice/uploader and @bob again"),
            ["bob/machine", "alice/uploader"]
        );
        assert_eq!(read("mail me at alice@example.com"), Vec::<String>::new());
        assert_eq!(read("@carol, @alice/billing, @Alice"), Vec::<String>::new());
    }
}
