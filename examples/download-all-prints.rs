use scryfall::Card;

fn main() -> scryfall::Result<()> {
    let card_name = std::env::args().nth(1).unwrap();

    let image_uris = Card::search(format!("!\"{}\" unique:prints", card_name).as_str())?
        .filter_map(|card| card.image_uris)
        .filter_map(|mut uris| uris.remove("normal"));

    for image in image_uris {
        println!("{}", image);
    }

    Ok(())
}
