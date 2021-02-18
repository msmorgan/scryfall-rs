use anyhow::anyhow;
use scraper::{ElementRef, Html, Selector};

const CARDS_URL: &str = "https://scryfall.com/docs/api/cards";

const CORE_CARD_FIELDS_SELECTOR: &str =
    "h2#core-card-fields ~ table.attributes-table:nth-of-type(1)";

fn get_text(e: ElementRef) -> String {
    e.text()
        .map(|node| node.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> anyhow::Result<()> {
    let html = ureq::get(CARDS_URL).call()?.into_string()?;

    let doc = scraper::Html::parse_document(&html);
    let core_fields_sel = scraper::Selector::parse(CORE_CARD_FIELDS_SELECTOR).unwrap();
    let core_fields_table = doc
        .select(&core_fields_sel)
        .next()
        .ok_or(anyhow!("Table not found."))?;

    let row_selector = scraper::Selector::parse("tbody tr").unwrap();
    for row in core_fields_table.select(&row_selector) {
        let cell_selector = scraper::Selector::parse("td").unwrap();
        let mut columns = row.select(&cell_selector);
        let property = get_text(columns.next().unwrap());
        let prop_type = get_text(columns.next().unwrap());
        let _ = columns.next();
        let details = get_text(columns.next().unwrap());

        println!("{} | {} | {}", property, prop_type, details);
    }

    Ok(())
}
